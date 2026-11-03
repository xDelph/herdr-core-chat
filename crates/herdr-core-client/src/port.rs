//! Adaptateur `ControlPort` pour herdr-core (P2c) : lecture seule.
//!
//! Mappe les terminaux herdr-core vers le modèle d'observation 0.9.3
//! (workspaces/onglets/panes/agents). Les capacités mutantes non encore
//! portées (prompt, send_text, send_keys, focus, create_tab, start_agent,
//! fermer_onglet, stop) renvoient une erreur nommée — jamais un substitut.

use crate::client::{ClientCore, Terminal};
use crate::disposition::disposition_depuis;
use domain::errors::PortError;
use domain::ids::{AgentKind, PaneId, TabId, WorkspaceId};
use domain::observation::{
    AgentObservation, ReadSource, ScreenObservation, SituatedPane, Snapshot, SnapshotProvenance,
    TabObservation, UpstreamPing, WorkspaceObservation,
};
use domain::ports::{
    ControlPort, CreateTab, CreatedTab, FocusEvidence, PartialStep, PortFuture, PromptEvidence,
    ScreenRead, StartAgent, StartEvidence, StopEvidence, TargetedKeys, TargetedPrompt,
    TargetedText, TransportEvidence, TransportOutcome,
};
use domain::status::AgentStatus;
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub struct PortCore {
    client: Arc<Mutex<ClientCore>>,
}

impl PortCore {
    pub fn nouveau(client: ClientCore) -> Self {
        Self {
            client: Arc::new(Mutex::new(client)),
        }
    }

    fn verrouiller(&self) -> Result<std::sync::MutexGuard<'_, ClientCore>, PortError> {
        self.client.lock().map_err(|_| PortError::Transport {
            detail: "verrou du client empoisonné".into(),
        })
    }

    fn agent_status_depuis(s: Option<&Value>) -> AgentStatus {
        match s.and_then(|v| v.get("state")).and_then(Value::as_str) {
            Some("idle") => AgentStatus::Idle,
            Some("working") => AgentStatus::Working,
            Some("blocked") => AgentStatus::Blocked,
            Some("done") => AgentStatus::Done,
            _ => AgentStatus::Unknown,
        }
    }

    fn kind_depuis(term: &Terminal) -> Option<AgentKind> {
        // status.app d'abord, puis le basename du premier argv.
        let app = term
            .status
            .as_ref()
            .and_then(|s| s.get("app"))
            .and_then(Value::as_str);
        let candidat = app.or_else(|| term.argv.first().map(String::as_str));
        let nom = candidat?.rsplit('/').next()?.to_string();
        if nom.is_empty() {
            None
        } else {
            Some(AgentKind::from(nom))
        }
    }

    fn construire_snapshot(&self) -> Result<Snapshot, PortError> {
        let mut client = self.verrouiller()?;
        let hello = client.hello().ok();
        let terms = client.terminal_list().map_err(|e| PortError::Transport {
            detail: e.to_string(),
        })?;
        let doc = client
            .doc_get("herdr-app")
            .map_err(|e| PortError::Transport {
                detail: e.to_string(),
            })?;
        let provenance = hello.map(|h| domain::observation::SnapshotProvenance {
            protocol: h.generation as u64,
            version: h.version,
        });
        drop(client);

        let disposition = doc.value.as_ref().and_then(|v| disposition_depuis(v).ok());

        // Index terminal → (space_id, tab_id) via disposition.
        let mut term_space: std::collections::BTreeMap<String, (String, String)> =
            std::collections::BTreeMap::new();
        if let Some(disp) = &disposition {
            for sp in &disp.spaces {
                for tb in &sp.onglets {
                    for t in &tb.terminaux {
                        term_space.insert(t.clone(), (sp.id.clone(), tb.id.clone()));
                    }
                }
            }
        }

        // Terminaux hors disposition → un espace synthétique.
        let mut hors: Vec<String> = Vec::new();
        for t in &terms.terminals {
            if !term_space.contains_key(&t.address) {
                hors.push(t.address.clone());
            }
        }

        // Workspaces
        let mut workspaces: Vec<WorkspaceObservation> = Vec::new();
        let mut tabs: Vec<TabObservation> = Vec::new();
        let mut panes: Vec<SituatedPane> = Vec::new();

        if let Some(disp) = &disposition {
            for sp in &disp.spaces {
                let mut pane_count = 0u64;
                let mut tab_count = 0u64;
                let mut agg_status: Option<AgentStatus> = None;
                for tb in &sp.onglets {
                    tab_count += 1;
                    let term_addrs = &tb.terminaux;
                    pane_count += term_addrs.len() as u64;
                    for ta in term_addrs {
                        if let Some(t) = terms.terminals.iter().find(|x| x.address == *ta) {
                            let st = Self::agent_status_depuis(t.status.as_ref());
                            agg_status = Some(match (agg_status, st) {
                                (None, s) => s,
                                (Some(AgentStatus::Blocked), _) => AgentStatus::Blocked,
                                (Some(AgentStatus::Working), AgentStatus::Blocked) => {
                                    AgentStatus::Blocked
                                }
                                (Some(AgentStatus::Working), _) => AgentStatus::Working,
                                (Some(AgentStatus::Done), AgentStatus::Blocked) => {
                                    AgentStatus::Blocked
                                }
                                (Some(AgentStatus::Done), AgentStatus::Working) => {
                                    AgentStatus::Working
                                }
                                (Some(s), _) => s,
                            });
                        }
                    }
                    tabs.push(TabObservation {
                        tab_id: TabId::from(tb.id.clone()),
                        workspace_id: WorkspaceId::from(sp.id.clone()),
                        label: if tb.name.is_empty() {
                            None
                        } else {
                            Some(tb.name.clone())
                        },
                        number: tabs.len() as u64 + 1,
                        agent_status: None,
                        pane_count: Some(term_addrs.len() as u64),
                    });
                }
                workspaces.push(WorkspaceObservation {
                    workspace_id: WorkspaceId::from(sp.id.clone()),
                    label: if sp.name.is_empty() {
                        Some(sp.folder.clone())
                    } else {
                        Some(sp.name.clone())
                    },
                    agent_status: agg_status,
                    focused: None,
                    number: None,
                    pane_count: Some(pane_count),
                    tab_count: Some(tab_count),
                    active_tab_id: None,
                    worktree: None,
                });
            }
        }

        // Espace synthétique pour les terminaux hors disposition.
        if !hors.is_empty() {
            let ws_id = "hors-disposition".to_string();
            let tab_id = "hors-disposition-tab".to_string();
            workspaces.push(WorkspaceObservation {
                workspace_id: WorkspaceId::from(ws_id.clone()),
                label: Some("Hors disposition".into()),
                agent_status: None,
                focused: None,
                number: None,
                pane_count: Some(hors.len() as u64),
                tab_count: Some(1),
                active_tab_id: None,
                worktree: None,
            });
            tabs.push(TabObservation {
                tab_id: TabId::from(tab_id.clone()),
                workspace_id: WorkspaceId::from(ws_id.clone()),
                label: Some("Hors disposition".into()),
                number: tabs.len() as u64 + 1,
                agent_status: None,
                pane_count: Some(hors.len() as u64),
            });
            for ta in &hors {
                term_space.insert(ta.clone(), (ws_id.clone(), tab_id.clone()));
            }
        }

        // Agents + panes
        let mut agents: Vec<AgentObservation> = Vec::new();
        for t in &terms.terminals {
            let (ws_id, tab_id) = term_space
                .get(&t.address)
                .cloned()
                .unwrap_or_else(|| ("hors-disposition".into(), "hors-disposition-tab".into()));
            panes.push(SituatedPane {
                pane_id: PaneId::from(t.address.clone()),
                workspace_id: Some(WorkspaceId::from(ws_id.clone())),
                cwd: Some(t.cwd.clone()),
            });
            if t.status.as_ref().map(|v| !v.is_null()).unwrap_or(false) {
                agents.push(AgentObservation {
                    pane_id: PaneId::from(t.address.clone()),
                    tab_id: TabId::from(tab_id.clone()),
                    workspace_id: WorkspaceId::from(ws_id.clone()),
                    terminal_id: t.address.clone(),
                    revision: t.turn,
                    status: Self::agent_status_depuis(t.status.as_ref()),
                    focused: false,
                    agent: Self::kind_depuis(t),
                    name: None,
                    terminal_title: if t.title.is_empty() {
                        None
                    } else {
                        Some(t.title.clone())
                    },
                    cwd: Some(t.cwd.clone()),
                    foreground_pid: t.foreground_pid,
                    foreground_cwd: None,
                    completion_seq: None,
                    state_change_seq: None,
                    session: None,
                });
            }
        }

        Ok(Snapshot {
            agents,
            workspaces,
            tabs: Some(tabs),
            layouts: None,
            focused_pane: None,
            focused_tab: None,
            focused_workspace: None,
            provenance,
            panes,
        })
    }
}

impl ControlPort for PortCore {
    fn ping(&self) -> PortFuture<'_, UpstreamPing> {
        let mut client = match self.verrouiller() {
            Ok(c) => c,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        match client.hello() {
            Ok(h) => Box::pin(async move {
                Ok(UpstreamPing {
                    protocol: h.generation as u64,
                    version: h.version,
                })
            }),
            Err(e) => Box::pin(async move {
                Err(PortError::Transport {
                    detail: e.to_string(),
                })
            }),
        }
    }
    // placeholder; replaced below
    fn snapshot(&self) -> PortFuture<'_, Snapshot> {
        let this = self;
        Box::pin(async move { this.construire_snapshot() })
    }
    fn list_agents(&self) -> PortFuture<'_, domain::observation::AgentListObservation> {
        match self.construire_snapshot() {
            Ok(snap) => Box::pin(async move {
                Ok(domain::observation::AgentListObservation {
                    agents: snap.agents,
                })
            }),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }
    fn read_screen(&self, request: ScreenRead) -> PortFuture<'_, ScreenObservation> {
        let term_addr = request.pane.to_string();
        let max_lines = request.max_lines;
        let mut client = match self.verrouiller() {
            Ok(c) => c,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        match client.terminal_read(&term_addr, max_lines) {
            Ok(text) => Box::pin(async move {
                Ok(ScreenObservation {
                    pane_id: PaneId::from(term_addr.clone()),
                    text: Some(text),
                    truncated: None,
                    source: Some(ReadSource::Visible),
                })
            }),
            Err(e) => Box::pin(async move {
                Err(PortError::Transport {
                    detail: e.to_string(),
                })
            }),
        }
    }
    fn prompt(&self, request: TargetedPrompt) -> PortFuture<'_, PromptEvidence> {
        let addr = request.pane.to_string();
        let text = request.literal_text.clone();
        let mut client = match self.verrouiller() {
            Ok(c) => c,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        match client.terminal_send(&addr, &text, None) {
            Ok((_turn, _status, _started)) => Box::pin(async {
                Ok(PromptEvidence::Unknown {
                    partial: vec![PartialStep::LiteralTextWritten],
                    detail: "herdr-core : pas de confirmation d'acceptation (send sans preuve)"
                        .into(),
                })
            }),
            Err(e) => Box::pin(async move {
                Ok(PromptEvidence::Failed {
                    detail: e.to_string(),
                })
            }),
        }
    }
    fn send_text(&self, request: TargetedText) -> PortFuture<'_, TransportEvidence> {
        let addr = request.pane.to_string();
        let text = request.text.clone();
        let mut client = match self.verrouiller() {
            Ok(c) => c,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        match client.terminal_send(&addr, &text, None) {
            Ok((_turn, _status, _started)) => Box::pin(async {
                Ok(TransportEvidence {
                    outcome: TransportOutcome::Unknown,
                    partial: vec![PartialStep::LiteralTextWritten],
                    detail: "herdr-core : texte envoyé, acceptation non confirmée".into(),
                })
            }),
            Err(e) => Box::pin(async move {
                Ok(TransportEvidence {
                    outcome: TransportOutcome::Refused,
                    partial: vec![],
                    detail: e.to_string(),
                })
            }),
        }
    }
    fn send_keys(&self, request: TargetedKeys) -> PortFuture<'_, TransportEvidence> {
        let addr = request.pane.to_string();
        let keys: Vec<String> = request.keys.clone();
        let mut client = match self.verrouiller() {
            Ok(c) => c,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        match client.terminal_keys(&addr, &keys) {
            Ok(()) => Box::pin(async {
                Ok(TransportEvidence {
                    outcome: TransportOutcome::Unknown,
                    partial: vec![PartialStep::LiteralTextWritten],
                    detail: "herdr-core : touches envoyées".into(),
                })
            }),
            Err(e) => Box::pin(async move {
                Ok(TransportEvidence {
                    outcome: TransportOutcome::Refused,
                    partial: vec![],
                    detail: e.to_string(),
                })
            }),
        }
    }
    fn focus(&self, _pane: PaneId) -> PortFuture<'_, FocusEvidence> {
        Box::pin(async {
            Err(PortError::Unsupported {
                feature: "focus".into(),
            })
        })
    }
    fn create_tab(&self, _request: CreateTab) -> PortFuture<'_, CreatedTab> {
        Box::pin(async {
            Err(PortError::Unsupported {
                feature: "create_tab".into(),
            })
        })
    }
    fn start_agent(&self, _request: StartAgent) -> PortFuture<'_, StartEvidence> {
        Box::pin(async {
            Err(PortError::Unsupported {
                feature: "start_agent".into(),
            })
        })
    }
    fn fermer_onglet(&self, _tab: TabId) -> PortFuture<'_, ()> {
        Box::pin(async {
            Err(PortError::Unsupported {
                feature: "fermer_onglet".into(),
            })
        })
    }
    fn stop(&self, pane: PaneId) -> PortFuture<'_, StopEvidence> {
        let addr = pane.to_string();
        let mut client = match self.verrouiller() {
            Ok(c) => c,
            Err(e) => return Box::pin(async move { Err(e) }),
        };
        match client.terminal_interrupt(&addr) {
            Ok(outcome) if outcome == "exited" || outcome == "stopped" => {
                Box::pin(async { Ok(StopEvidence::InterruptSent) })
            }
            Ok(_) => Box::pin(async { Ok(StopEvidence::NotRunning) }),
            Err(e) => Box::pin(async move {
                Err(PortError::Transport {
                    detail: e.to_string(),
                })
            }),
        }
    }
}
