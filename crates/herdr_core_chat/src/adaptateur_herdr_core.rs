//! Adaptateur herdr-core — **stub provisoire (P0)**.
//!
//! Remplace l'ancien `herdr-client` (Herdr 0.9.3). L'implémentation réelle
//! viendra en P2 (transport JSON-RPC + port de contrôle). En attendant, le
//! démon démarre mais tout appel de contrôle échoue avec une cause nommée :
//! aucune donnée inventée, aucune panne masquée.

use application::noyau::PortError;
use application::noyau::{AgentListObservation, ScreenObservation, Snapshot, UpstreamPing};
use application::noyau::{PaneId, TabId};
use application::ports::ControlPort;
use application::ports::{CreateTab, TargetedKeys, TargetedPrompt, TargetedText};
use application::ports::{CreatedTab, FocusEvidence, PromptEvidence, ScreenRead};
use application::ports::{StartAgent, StartEvidence, StopEvidence, TransportEvidence};
use std::future::Future;
use std::pin::Pin;

/// Type alias local pour rester lisible.
type PortFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, PortError>> + Send + 'a>>;

/// Adaptateur herdr-core non implémenté (P0/P2). Toute capacité de contrôle
/// est signalée absente, jamais inventée.
pub struct AdaptateurHerdrCoreStub;

impl ControlPort for AdaptateurHerdrCoreStub {
    fn ping(&self) -> PortFuture<'_, UpstreamPing> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn snapshot(&self) -> PortFuture<'_, Snapshot> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn list_agents(&self) -> PortFuture<'_, AgentListObservation> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn read_screen(&self, _request: ScreenRead) -> PortFuture<'_, ScreenObservation> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn prompt(&self, _request: TargetedPrompt) -> PortFuture<'_, PromptEvidence> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn send_text(&self, _request: TargetedText) -> PortFuture<'_, TransportEvidence> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn send_keys(&self, _request: TargetedKeys) -> PortFuture<'_, TransportEvidence> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn focus(&self, _pane: PaneId) -> PortFuture<'_, FocusEvidence> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn create_tab(&self, _request: CreateTab) -> PortFuture<'_, CreatedTab> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn start_agent(&self, _request: StartAgent) -> PortFuture<'_, StartEvidence> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn fermer_onglet(&self, _tab: TabId) -> PortFuture<'_, ()> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
    fn stop(&self, _pane: PaneId) -> PortFuture<'_, StopEvidence> {
        Box::pin(async {
            Err(PortError::Transport {
                detail: String::from("adaptateur herdr-core non implémenté (P2)"),
            })
        })
    }
}
