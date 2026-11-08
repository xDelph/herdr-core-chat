//! Adaptateur `ControlPort` pour herdr-core.
//!
//! Mappe les terminaux herdr-core vers le modèle d'observation du domaine
//! (espaces → onglets → terminaux, vus comme workspaces/tabs/panes).
//!
//! ⚠️ **Port bloquant-déguisé** : les appelants (`interactions`,
//! `application`) sondent chaque futur UNE fois et exigent une réponse
//! immédiate. Chaque méthode fait donc son I/O de socket avant de rendre un
//! futur déjà prêt — jamais un `spawn_blocking`.
//!
//! ⚠️ **Rien n'est masqué** : une disposition illisible devient un espace
//! « Herdr.app layout unavailable » qui porte la cause, un état Herdr.app
//! illisible est journalisé, une génération de protocole inconnue est
//! refusée, une erreur de l'amont garde son `data.kind`.
//!
//! ⚠️ **Lancement (P7)** — comme Herdr.app (`tab.new` : « Agents run in your
//! interactive shell ») : `create_tab` lance **le shell de la machine** dans
//! un nouvel onglet de l'espace (labels `herdr.space`/`herdr.tab` : « a
//! terminal with those labels shows up there », guide de Herdr.app),
//! `start_agent` y tape la commande de l'agent, `fermer_onglet` ne ferme que
//! les onglets **créés ici**. On ne tape jamais dans un terminal qu'on n'a
//! pas créé (AGENTS §1.1). `focus` reste `Unsupported` (pas de fenêtre
//! côté core).

use crate::client::{ClientCore, HelloResult, Terminal};
use crate::disposition::{Disposition, disposition_depuis};
use crate::transport::ErreurTransport;
use crate::workspace_state::WorkspaceState;
use domain::errors::PortError;
use domain::ids::{AgentKind, PaneId, TabId, WorkspaceId};
use domain::observation::{
    AgentListObservation, AgentObservation, ReadSource, ScreenObservation, SituatedPane, Snapshot,
    SnapshotProvenance, TabObservation, UpstreamPing, WorkspaceObservation,
};
use domain::ports::{
    ControlPort, CreateTab, CreatedTab, FocusEvidence, PartialStep, PortFuture, PromptEvidence,
    ScreenRead, StartAgent, StartEvidence, StopEvidence, TargetedKeys, TargetedPrompt,
    TargetedText, TransportEvidence, TransportOutcome,
};
use domain::status::AgentStatus;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// La seule génération de protocole herdr-core vérifiée (`hello`, 0.1.17).
pub const GENERATION_SUPPORTEE: u32 = 1;

/// Le nom du doc où Herdr.app range sa disposition.
pub const DOC_DISPOSITION: &str = "herdr-app";

/// Les programmes reconnus comme agents quand ils ne publient aucun statut
/// (nom du programme au premier plan) — le type reste celui du programme,
/// le statut reste `unknown`.
const AGENTS_CONNUS: [&str; 6] = ["pi", "claude", "codex", "opencode", "devin", "cursor-agent"];

/// Fenêtre pendant laquelle un `enter` qui suit un `send_text` sur le même
/// terminal est considéré comme déjà appuyé (voir `send_keys`).
const FENETRE_ENTREE_DEJA_APPUYEE: Duration = Duration::from_secs(5);

/// Identifiant de l'espace synthétique quand la disposition est illisible.
pub const ESPACE_INDISPONIBLE: &str = "herdr-app-layout-unavailable";

/// Préfixe des espaces synthétiques « hors disposition ».
pub const PREFIXE_HORS: &str = "hors:";

/// Combien de temps on observe l'amorçage d'un agent après avoir tapé sa
/// commande (le moteur de lancement borne le tout à 125 s).
pub const DELAI_AMORCAGE: Duration = Duration::from_secs(20);

/// Le pas d'observation de l'amorçage.
const PAS_AMORCAGE: Duration = Duration::from_millis(500);

/// L'observation qui prolonge celle de `terminal.send`.
///
/// ⚠️ `terminal.send` n'attend le passage à `working` que **≈5 s** (mesuré
/// le 2026-11-08, herdr-core 0.1.18, faux agent sur core isolé : démarrage à
/// 3,5 s → `started: true` en 3,7 s ; à 5 s ou 8 s → `started: false` en
/// 5,2 s). Un agent plus lent à publier `working` (gros contexte, réflexion)
/// était affiché « not confirmed » alors qu'il avait bien pris le message.
pub const DELAI_CONFIRMATION: Duration = Duration::from_secs(6);

/// Le pas de cette observation.
const PAS_CONFIRMATION: Duration = Duration::from_millis(500);

static COMPTEUR_ONGLETS: AtomicU64 = AtomicU64::new(0);

/// Un onglet que CE démon a créé : son terminal (le shell), seul cible
/// permise de `start_agent` et `fermer_onglet`.
#[derive(Clone, Debug)]
struct Lance {
    adresse: String,
}

pub struct PortCore {
    client: Mutex<ClientCore>,
    /// Le socket du core : l'envoi y ouvre son propre client.
    socket: PathBuf,
    etat_herdr_app: PathBuf,
    /// Dernières causes journalisées — une cause n'est dite qu'à son
    /// apparition, pas toutes les 2 s.
    causes_dites: Mutex<BTreeMap<&'static str, String>>,
    /// Le dernier `send_text` : terminal et instant.
    dernier_texte: Mutex<Option<(String, Instant)>>,
    /// Les onglets créés par ce démon (identifiant d'onglet → terminal).
    lances: Mutex<BTreeMap<String, Lance>>,
    /// Le délai d'observation de l'amorçage (les tests le raccourcissent).
    delai_amorcage: Duration,
    /// Le délai d'observation après un `started: false` (idem).
    delai_confirmation: Duration,
}

impl PortCore {
    /// `etat_herdr_app` : le chemin de `workspace-state.json` (injecté par la
    /// configuration — la bibliothèque ne lit pas `HOME`).
    pub fn nouveau(client: ClientCore, etat_herdr_app: impl AsRef<Path>) -> Self {
        Self {
            socket: client.chemin().to_path_buf(),
            client: Mutex::new(client),
            etat_herdr_app: etat_herdr_app.as_ref().to_path_buf(),
            causes_dites: Mutex::new(BTreeMap::new()),
            dernier_texte: Mutex::new(None),
            lances: Mutex::new(BTreeMap::new()),
            delai_amorcage: DELAI_AMORCAGE,
            delai_confirmation: DELAI_CONFIRMATION,
        }
    }

    /// Le même port avec un délai de confirmation choisi (tests).
    #[must_use]
    pub fn avec_delai_confirmation(mut self, delai: Duration) -> Self {
        self.delai_confirmation = delai;
        self
    }

    /// L'agent a-t-il pris le message **après** la fenêtre de
    /// `terminal.send` ? Preuve : il passe `working`, ou son tour avance
    /// (il est entré puis sorti de `working` entre deux lectures). Une
    /// lecture qui échoue, un terminal disparu ou fini : rien n'est prouvé.
    fn demarre_apres(&self, adresse: &str, tour: u64) -> bool {
        let debut = Instant::now();
        loop {
            let Ok(terminal) = self.terminal(adresse) else {
                return false;
            };
            if terminal.state == "finished" {
                return false;
            }
            let travaille = terminal
                .status
                .as_ref()
                .and_then(|status| status.get("state"))
                .and_then(Value::as_str)
                == Some("working");
            if travaille || terminal.turn > tour {
                return true;
            }
            if debut.elapsed() >= self.delai_confirmation {
                return false;
            }
            std::thread::sleep(PAS_CONFIRMATION);
        }
    }

    /// Le même port avec un délai d'amorçage choisi (tests).
    #[must_use]
    pub fn avec_delai_amorcage(mut self, delai: Duration) -> Self {
        self.delai_amorcage = delai;
        self
    }

    /// Le terminal d'un onglet créé ici, s'il l'a été.
    fn lance_de_terminal(&self, adresse: &str) -> Option<Lance> {
        self.lances
            .lock()
            .ok()?
            .values()
            .find(|lance| lance.adresse == adresse)
            .cloned()
    }

    fn verrouiller(&self) -> Result<MutexGuard<'_, ClientCore>, PortError> {
        self.client.lock().map_err(|_| PortError::Transport {
            detail: "herdr-core client lock poisoned".into(),
        })
    }

    /// Journalise une cause à son apparition (et sa disparition).
    fn dit_cause(&self, sujet: &'static str, cause: Option<&str>) {
        let Ok(mut dites) = self.causes_dites.lock() else {
            return;
        };
        match cause {
            Some(cause) => {
                if dites.get(sujet).map(String::as_str) != Some(cause) {
                    tracing::warn!(sujet, cause, "herdr-core : source dégradée");
                    dites.insert(sujet, cause.to_string());
                }
            }
            None => {
                if dites.remove(sujet).is_some() {
                    tracing::info!(sujet, "herdr-core : source rétablie");
                }
            }
        }
    }

    /// Le mot de statut 7501 → l'état du domaine. Un mot hors de la liste
    /// est `Unknown`, jamais rapproché d'un voisin.
    #[must_use]
    pub fn agent_status_depuis(status: Option<&Value>) -> AgentStatus {
        match status.and_then(|v| v.get("state")).and_then(Value::as_str) {
            Some("idle") => AgentStatus::Idle,
            Some("working") => AgentStatus::Working,
            Some("blocked") => AgentStatus::Blocked,
            Some("done") => AgentStatus::Done,
            Some("error") => AgentStatus::Error,
            _ => AgentStatus::Unknown,
        }
    }

    /// Le type d'agent : `status.app` (le programme le dit), sinon le
    /// programme au premier plan s'il est un agent connu — par son nom ou par
    /// le `argv[0]` du premier plan (cursor-agent tourne sous `node`). Jamais
    /// l'`argv` du terminal (c'est son shell).
    #[must_use]
    pub fn kind_depuis(term: &Terminal) -> Option<AgentKind> {
        let app = term
            .status
            .as_ref()
            .and_then(|s| s.get("app"))
            .and_then(Value::as_str)
            .filter(|app| !app.is_empty());
        let nom = match app {
            // herdr-core reporte « claude-code » ; le type du domaine est « claude ».
            Some("claude-code") => "claude",
            Some(app) => app,
            None => [
                term.foreground_name.as_deref(),
                term.foreground_argv.first().map(String::as_str),
            ]
            .into_iter()
            .flatten()
            .map(|nom| nom.rsplit('/').next().unwrap_or(nom))
            .find(|nom| AGENTS_CONNUS.contains(nom))?,
        };
        // La commande de Cursor est `cursor-agent`, son type `cursor`.
        let nom = if nom == "cursor-agent" { "cursor" } else { nom };
        Some(AgentKind::from(nom.to_string()))
    }

    /// Un terminal est un agent s'il publie un statut, ou si son programme
    /// au premier plan est un agent connu (statut `unknown`).
    fn est_agent(term: &Terminal) -> bool {
        term.status.as_ref().is_some_and(|v| !v.is_null()) || Self::kind_depuis(term).is_some()
    }

    fn hello_verifie(client: &mut ClientCore) -> Result<HelloResult, PortError> {
        let hello = client.hello().map_err(erreur_port)?;
        if hello.generation != GENERATION_SUPPORTEE {
            return Err(PortError::Unsupported {
                feature: format!(
                    "herdr-core protocol generation {} (supported: {GENERATION_SUPPORTEE})",
                    hello.generation
                ),
            });
        }
        Ok(hello)
    }

    fn construire_snapshot(&self) -> Result<Snapshot, PortError> {
        let mut client = self.verrouiller()?;
        let hello = Self::hello_verifie(&mut client)?;
        let terms = client.terminal_list().map_err(erreur_port)?;
        let doc = client.doc_get(DOC_DISPOSITION).map_err(erreur_port)?;
        drop(client);

        let disposition = match doc.value.as_ref().map(disposition_depuis) {
            None => {
                self.dit_cause("disposition", None);
                Disposition { spaces: Vec::new() }
            }
            Some(Ok(disposition)) => {
                self.dit_cause("disposition", None);
                disposition
            }
            Some(Err(cause)) => {
                let cause = format!("doc `{DOC_DISPOSITION}` v{}: {cause}", doc.version);
                self.dit_cause("disposition", Some(&cause));
                return Ok(snapshot_sans_disposition(&terms.terminals, &cause, hello));
            }
        };
        let etat = match WorkspaceState::charger(&self.etat_herdr_app) {
            Ok(etat) => {
                self.dit_cause("workspace-state", None);
                etat
            }
            Err(cause) => {
                self.dit_cause("workspace-state", Some(&cause));
                None
            }
        };
        Ok(snapshot_depuis(
            &terms.terminals,
            &disposition,
            etat.as_ref(),
            hello,
        ))
    }

    /// Le terminal tel que le core le voit maintenant.
    fn terminal(&self, adresse: &str) -> Result<Terminal, PortError> {
        let mut client = self.verrouiller()?;
        let terms = client.terminal_list().map_err(erreur_port)?;
        terms
            .terminals
            .into_iter()
            .find(|t| t.address == adresse)
            .ok_or_else(|| PortError::Upstream {
                code: "not_found".into(),
                message: format!("terminal {adresse} is not on this core"),
            })
    }
}

/// Une erreur du transport → l'erreur du port, **cause et `data.kind`
/// gardés**.
#[must_use]
pub fn erreur_port(erreur: ErreurTransport) -> PortError {
    match erreur {
        ErreurTransport::AmontRefus {
            code,
            message,
            kind,
        } => PortError::Upstream {
            code: kind.unwrap_or_else(|| code.to_string()),
            message,
        },
        ErreurTransport::DelaiDepasse => PortError::Timeout,
        autre => PortError::Transport {
            detail: autre.to_string(),
        },
    }
}

fn provenance(hello: HelloResult) -> SnapshotProvenance {
    SnapshotProvenance {
        protocol: u64::from(hello.generation),
        version: hello.version,
    }
}

/// Le rang d'un état pour l'agrégat d'un espace : ce qui demande
/// l'utilisateur d'abord.
fn rang(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 5,
        AgentStatus::Error => 4,
        AgentStatus::Working => 3,
        AgentStatus::Done => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}

fn agrege(courant: Option<AgentStatus>, nouveau: AgentStatus) -> Option<AgentStatus> {
    match courant {
        Some(courant) if rang(courant) >= rang(nouveau) => Some(courant),
        _ => Some(nouveau),
    }
}

fn dossier_du(term: &Terminal) -> String {
    term.foreground_cwd
        .clone()
        .filter(|cwd| !cwd.is_empty())
        .unwrap_or_else(|| term.cwd.clone())
}

fn nom_de_dossier(chemin: &str) -> String {
    Path::new(chemin)
        .file_name()
        .map(|nom| nom.to_string_lossy().into_owned())
        .filter(|nom| !nom.is_empty())
        .unwrap_or_else(|| chemin.to_string())
}

fn agent_de(term: &Terminal, espace: &str, onglet: &str) -> AgentObservation {
    AgentObservation {
        pane_id: PaneId::from(term.address.clone()),
        tab_id: TabId::from(onglet.to_string()),
        workspace_id: WorkspaceId::from(espace.to_string()),
        terminal_id: term.address.clone(),
        revision: term.turn,
        status: PortCore::agent_status_depuis(term.status.as_ref()),
        focused: false,
        agent: PortCore::kind_depuis(term),
        name: term
            .name
            .clone()
            .filter(|nom| !nom.is_empty())
            .map(Into::into),
        terminal_title: Some(term.title.clone()).filter(|titre| !titre.is_empty()),
        cwd: Some(dossier_du(term)),
        foreground_pid: term.foreground_pid,
        foreground_cwd: term.foreground_cwd.clone(),
        completion_seq: None,
        state_change_seq: None,
        session: None,
    }
}

/// La disposition illisible : UN espace qui porte la cause, tous les
/// terminaux dedans — rien n'est reconstruit au hasard.
fn snapshot_sans_disposition(terms: &[Terminal], cause: &str, hello: HelloResult) -> Snapshot {
    let onglet = format!("{ESPACE_INDISPONIBLE}:tab");
    let mut snapshot = Snapshot {
        provenance: Some(provenance(hello)),
        ..Snapshot::default()
    };
    let mut statut = None;
    for term in terms {
        snapshot.panes.push(SituatedPane {
            pane_id: PaneId::from(term.address.clone()),
            workspace_id: Some(WorkspaceId::from(ESPACE_INDISPONIBLE.to_string())),
            cwd: Some(dossier_du(term)),
        });
        if PortCore::est_agent(term) {
            let agent = agent_de(term, ESPACE_INDISPONIBLE, &onglet);
            statut = agrege(statut, agent.status);
            snapshot.agents.push(agent);
        }
    }
    snapshot.workspaces.push(WorkspaceObservation {
        workspace_id: WorkspaceId::from(ESPACE_INDISPONIBLE.to_string()),
        label: Some(format!("Herdr.app layout unavailable: {cause}")),
        agent_status: statut,
        focused: None,
        number: None,
        pane_count: Some(terms.len() as u64),
        tab_count: Some(1),
        active_tab_id: None,
        worktree: None,
    });
    snapshot.tabs = Some(vec![TabObservation {
        tab_id: TabId::from(onglet),
        workspace_id: WorkspaceId::from(ESPACE_INDISPONIBLE.to_string()),
        label: None,
        number: 1,
        agent_status: None,
        pane_count: Some(terms.len() as u64),
    }]);
    snapshot
}

/// Un groupe d'espace : son identifiant, son nom, ses onglets dans l'ordre.
struct Groupe {
    id: String,
    label: Option<String>,
    onglets: Vec<(String, Option<String>, Vec<String>)>,
}

/// Le snapshot depuis la disposition de Herdr.app.
///
/// Un terminal hors disposition (ex. sous-agent `hc run`) reste visible :
/// dans l'espace de son parent s'il y est, sinon groupé par dossier — dans
/// un onglet « Outside Herdr.app ».
#[must_use]
pub fn snapshot_depuis(
    terms: &[Terminal],
    disposition: &Disposition,
    etat: Option<&WorkspaceState>,
    hello: HelloResult,
) -> Snapshot {
    let connus: BTreeMap<&str, &Terminal> = terms.iter().map(|t| (t.address.as_str(), t)).collect();
    let mut groupes: Vec<Groupe> = Vec::new();
    let mut place: BTreeMap<String, (String, String)> = BTreeMap::new();

    for espace in &disposition.spaces {
        let nom_etat = etat
            .and_then(|e| e.spaces.get(&espace.id))
            .map(|e| e.name.clone())
            .filter(|nom| !nom.is_empty());
        let label = Some(espace.name.clone())
            .filter(|nom| !nom.is_empty())
            .or(nom_etat)
            // Dernier recours (Herdr.app fermé, sans son état) : le NOM du
            // dossier, pas son chemin complet — il sert aussi de base au nom
            // d'agent lancé (`<nom>-1`).
            .or_else(|| {
                Some(espace.folder.clone())
                    .filter(|d| !d.is_empty())
                    .map(|d| nom_de_dossier(&d))
            });
        let mut onglets = Vec::new();
        for onglet in &espace.onglets {
            let nom_etat = etat
                .and_then(|e| e.tabs.get(&onglet.id))
                .map(|e| e.name.clone())
                .filter(|nom| !nom.is_empty());
            let label = Some(onglet.name.clone())
                .filter(|nom| !nom.is_empty())
                .or(nom_etat);
            let presents: Vec<String> = onglet
                .terminaux
                .iter()
                .filter(|adresse| connus.contains_key(adresse.as_str()))
                .cloned()
                .collect();
            for adresse in &presents {
                place.insert(adresse.clone(), (espace.id.clone(), onglet.id.clone()));
            }
            onglets.push((onglet.id.clone(), label, presents));
        }
        groupes.push(Groupe {
            id: espace.id.clone(),
            label,
            onglets,
        });
    }

    // Hors disposition : l'espace du parent, sinon le dossier.
    for term in terms {
        if place.contains_key(&term.address) {
            continue;
        }
        let espace_parent = term
            .parent
            .as_deref()
            .and_then(|parent| place.get(parent))
            .map(|(espace, _)| espace.clone());
        let id_espace = match espace_parent {
            Some(espace) => espace,
            None => {
                let dossier = dossier_du(term);
                let id = format!("hors:{dossier}");
                if !groupes.iter().any(|g| g.id == id) {
                    groupes.push(Groupe {
                        id: id.clone(),
                        label: Some(format!("{} (outside Herdr.app)", nom_de_dossier(&dossier))),
                        onglets: Vec::new(),
                    });
                }
                id
            }
        };
        let id_onglet = format!("hors:{id_espace}:tab");
        let Some(groupe) = groupes.iter_mut().find(|g| g.id == id_espace) else {
            continue;
        };
        match groupe
            .onglets
            .iter_mut()
            .find(|(id, _, _)| *id == id_onglet)
        {
            Some((_, _, presents)) => presents.push(term.address.clone()),
            None => groupe.onglets.push((
                id_onglet.clone(),
                Some(String::from("Outside Herdr.app")),
                vec![term.address.clone()],
            )),
        }
        place.insert(term.address.clone(), (id_espace, id_onglet));
    }

    let mut snapshot = Snapshot {
        provenance: Some(provenance(hello)),
        tabs: Some(Vec::new()),
        ..Snapshot::default()
    };
    for groupe in &groupes {
        let mut statut = None;
        let mut panes = 0u64;
        for (numero, (id_onglet, label, presents)) in groupe.onglets.iter().enumerate() {
            panes += presents.len() as u64;
            for adresse in presents {
                if let Some(term) = connus.get(adresse.as_str())
                    && PortCore::est_agent(term)
                {
                    statut = agrege(statut, PortCore::agent_status_depuis(term.status.as_ref()));
                }
            }
            if let Some(tabs) = snapshot.tabs.as_mut() {
                tabs.push(TabObservation {
                    tab_id: TabId::from(id_onglet.clone()),
                    workspace_id: WorkspaceId::from(groupe.id.clone()),
                    label: label.clone(),
                    number: numero as u64 + 1,
                    agent_status: None,
                    pane_count: Some(presents.len() as u64),
                });
            }
        }
        snapshot.workspaces.push(WorkspaceObservation {
            workspace_id: WorkspaceId::from(groupe.id.clone()),
            label: groupe.label.clone(),
            agent_status: statut,
            focused: None,
            number: None,
            pane_count: Some(panes),
            tab_count: Some(groupe.onglets.len() as u64),
            active_tab_id: None,
            worktree: None,
        });
    }

    for term in terms {
        let Some((espace, onglet)) = place.get(&term.address) else {
            continue;
        };
        snapshot.panes.push(SituatedPane {
            pane_id: PaneId::from(term.address.clone()),
            workspace_id: Some(WorkspaceId::from(espace.clone())),
            cwd: Some(dossier_du(term)),
        });
        if PortCore::est_agent(term) {
            snapshot.agents.push(agent_de(term, espace, onglet));
        }
    }
    snapshot
}

/// Un nouvel identifiant d'onglet, à la forme de ceux de Herdr.app
/// (`tb` + 32 hexadécimaux) : « an id Herdr doesn't know becomes a new tab ».
fn nouvel_onglet() -> String {
    use std::hash::{BuildHasher, Hasher};
    let n = COMPTEUR_ONGLETS.fetch_add(1, Ordering::SeqCst);
    let instant = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut moities = [0u64; 2];
    for (i, moitie) in moities.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(instant);
        h.write_u64(n);
        h.write_u32(std::process::id());
        h.write_usize(i);
        *moitie = h.finish();
    }
    format!("tb{:016x}{:016x}", moities[0], moities[1])
}

/// Un nom de terminal valide pour herdr-core (« 1-32 of a-z 0-9 - _ . »),
/// dérivé du nom d'agent ; `None` s'il n'en reste rien.
#[must_use]
pub fn nom_de_terminal(nom: &str) -> Option<String> {
    let propre: String = nom
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    let propre = propre.trim_matches('-').to_string();
    (!propre.is_empty()).then_some(propre)
}

/// La commande d'un type d'agent (Cursor : `cursor-agent`).
fn commande_de(kind: &str) -> &str {
    match kind {
        "cursor" => "cursor-agent",
        autre => autre,
    }
}

/// Un mot pour `sh`/`zsh`/`bash`/`fish` : tel quel s'il est sûr, sinon entre
/// apostrophes.
fn mot_shell(mot: &str) -> String {
    let sur = !mot.is_empty()
        && mot
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c));
    if sur {
        mot.to_string()
    } else {
        format!("'{}'", mot.replace('\'', "'\\''"))
    }
}

/// La ligne tapée dans le shell pour lancer l'agent.
#[must_use]
pub fn ligne_de_lancement(kind: &str, args: &[String]) -> String {
    std::iter::once(commande_de(kind))
        .chain(args.iter().map(String::as_str))
        .map(mot_shell)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Le futur déjà prêt d'un port bloquant-déguisé.
fn pret<'a, T: Send + 'a>(valeur: Result<T, PortError>) -> PortFuture<'a, T> {
    Box::pin(async move { valeur })
}

/// La preuve d'un `terminal.send` pour un prompt : `started` vrai est la
/// seule confirmation — le programme est passé `working` après l'envoi.
#[must_use]
pub fn preuve_de_prompt(
    envoi: &crate::client::EnvoiCore,
    fingerprint: domain::ids::DeliveryFingerprint,
) -> PromptEvidence {
    match envoi.started {
        Some(true) => PromptEvidence::Confirmed { fingerprint },
        Some(false) => PromptEvidence::Unknown {
            partial: vec![
                PartialStep::LiteralTextWritten,
                PartialStep::EnterSequenceSent,
            ],
            detail: format!(
                "text submitted at turn {} (status {}), but the agent did not start working",
                envoi.turn,
                envoi.status.as_deref().unwrap_or("unreported")
            ),
        },
        None => PromptEvidence::Unknown {
            partial: vec![
                PartialStep::LiteralTextWritten,
                PartialStep::EnterSequenceSent,
            ],
            detail: format!(
                "text submitted at turn {}; the program reports no status, so acceptance cannot be confirmed",
                envoi.turn
            ),
        },
    }
}

impl ControlPort for PortCore {
    fn ping(&self) -> PortFuture<'_, UpstreamPing> {
        let resultat = self.verrouiller().and_then(|mut client| {
            Self::hello_verifie(&mut client).map(|hello| UpstreamPing {
                protocol: u64::from(hello.generation),
                version: hello.version,
            })
        });
        pret(resultat)
    }

    fn snapshot(&self) -> PortFuture<'_, Snapshot> {
        pret(self.construire_snapshot())
    }

    fn list_agents(&self) -> PortFuture<'_, AgentListObservation> {
        pret(
            self.construire_snapshot()
                .map(|snapshot| AgentListObservation {
                    agents: snapshot.agents,
                }),
        )
    }

    fn read_screen(&self, request: ScreenRead) -> PortFuture<'_, ScreenObservation> {
        let adresse = request.pane.to_string();
        let resultat = self.verrouiller().and_then(|mut client| {
            client
                .terminal_read(&adresse, request.max_lines)
                .map_err(erreur_port)
        });
        pret(resultat.map(|text| ScreenObservation {
            pane_id: request.pane.clone(),
            text: Some(text),
            truncated: None,
            source: Some(ReadSource::Visible),
        }))
    }

    /// Le prompt : `terminal.send` sans `force` — un agent bloqué (décision
    /// de permission, question) refuse, et le refus remonte tel quel.
    ///
    /// ⚠️ **Un client à part** : `terminal.send` dure jusqu'à ~5 s (il attend
    /// le démarrage) ; tenir le client partagé bloquait la veille et l'accueil
    /// pendant tout ce temps. Chaque appel ouvre de toute façon sa connexion.
    ///
    /// Chaque envoi est journalisé (jamais le texte : c'est du contenu) avec
    /// ce que le core en a dit — c'est la trace qui permet d'expliquer un
    /// « not confirmed ».
    fn prompt(&self, request: TargetedPrompt) -> PortFuture<'_, PromptEvidence> {
        let adresse = request.pane.to_string();
        let debut = Instant::now();
        let mut client = ClientCore::nouveau(&self.socket);
        let preuve = match client.terminal_send(&adresse, &request.literal_text, None, false) {
            Ok(envoi) => {
                let rattrape =
                    envoi.started == Some(false) && self.demarre_apres(&adresse, envoi.turn);
                let preuve = if rattrape {
                    PromptEvidence::Confirmed {
                        fingerprint: request.fingerprint.clone(),
                    }
                } else {
                    preuve_de_prompt(&envoi, request.fingerprint.clone())
                };
                tracing::info!(
                    terminal = %adresse,
                    tour = envoi.turn,
                    statut = envoi.status.as_deref().unwrap_or("unreported"),
                    started = ?envoi.started,
                    rattrape,
                    confirme = matches!(preuve, PromptEvidence::Confirmed { .. }),
                    duree_ms = u64::try_from(debut.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "envoi : terminal.send"
                );
                preuve
            }
            Err(erreur) => {
                let detail = erreur_port(erreur).description();
                tracing::warn!(
                    terminal = %adresse,
                    cause = %detail,
                    duree_ms = u64::try_from(debut.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "envoi : terminal.send refusé"
                );
                PromptEvidence::Failed { detail }
            }
        };
        pret(Ok(preuve))
    }

    /// Le texte littéral : herdr-core n'a pas d'écriture sans Entrée —
    /// `terminal.send` colle PUIS appuie sur Entrée. Les deux étapes sont
    /// déclarées, et l'`enter` que l'appelant envoie ensuite est absorbé
    /// (`send_keys`).
    ///
    /// ⚠️ `force` seulement pour répondre à une **question** (`blocked` +
    /// `kind: question`) : jamais pour une permission ou une authentification.
    fn send_text(&self, request: TargetedText) -> PortFuture<'_, TransportEvidence> {
        let adresse = request.pane.to_string();
        let terminal = match self.terminal(&adresse) {
            Ok(terminal) => terminal,
            Err(erreur) => return pret(Err(erreur)),
        };
        let question = terminal.status.as_ref().is_some_and(|status| {
            status.get("state").and_then(Value::as_str) == Some("blocked")
                && status.get("kind").and_then(Value::as_str) == Some("question")
        });
        let resultat = self.verrouiller().map(|mut client| {
            match client.terminal_send(&adresse, &request.text, None, question) {
                Ok(envoi) => {
                    if let Ok(mut dernier) = self.dernier_texte.lock() {
                        *dernier = Some((adresse.clone(), Instant::now()));
                    }
                    TransportEvidence {
                        outcome: TransportOutcome::Delivered,
                        partial: vec![
                            PartialStep::LiteralTextWritten,
                            PartialStep::EnterSequenceSent,
                        ],
                        detail: format!("pasted and submitted at turn {}", envoi.turn),
                    }
                }
                Err(erreur) => TransportEvidence {
                    outcome: TransportOutcome::Refused,
                    partial: vec![],
                    detail: erreur_port(erreur).description(),
                },
            }
        });
        pret(resultat)
    }

    /// Les touches nommées (`terminal.keys`). Un `enter` SEUL qui suit de
    /// près un `send_text` sur le même terminal est déjà appuyé par
    /// `terminal.send` : il n'est pas renvoyé (un second Entrée validerait
    /// une ligne vide ou le choix suivant).
    fn send_keys(&self, request: TargetedKeys) -> PortFuture<'_, TransportEvidence> {
        let adresse = request.pane.to_string();
        let deja = request.keys == ["enter"]
            && self.dernier_texte.lock().ok().is_some_and(|mut dernier| {
                let recent = dernier.as_ref().is_some_and(|(terminal, instant)| {
                    *terminal == adresse && instant.elapsed() < FENETRE_ENTREE_DEJA_APPUYEE
                });
                if recent {
                    *dernier = None;
                }
                recent
            });
        if deja {
            return pret(Ok(TransportEvidence {
                outcome: TransportOutcome::Delivered,
                partial: vec![PartialStep::EnterSequenceSent],
                detail: "enter already pressed by terminal.send".into(),
            }));
        }
        let resultat = self.verrouiller().map(|mut client| {
            match client.terminal_keys(&adresse, &request.keys) {
                Ok(()) => TransportEvidence {
                    outcome: TransportOutcome::Delivered,
                    partial: vec![PartialStep::KeysSent],
                    detail: format!("keys pressed: {}", request.keys.join(" ")),
                },
                Err(erreur) => TransportEvidence {
                    outcome: TransportOutcome::Refused,
                    partial: vec![],
                    detail: erreur_port(erreur).description(),
                },
            }
        });
        pret(resultat)
    }

    fn focus(&self, _pane: PaneId) -> PortFuture<'_, FocusEvidence> {
        pret(Err(PortError::Unsupported {
            feature: "focus (herdr-core has no window focus)".into(),
        }))
    }

    /// Un nouvel onglet dans un espace de Herdr.app, avec le shell de la
    /// machine dans le dossier connu. Refusé hors d'un espace réel
    /// (terminaux hors disposition, disposition illisible).
    fn create_tab(&self, request: CreateTab) -> PortFuture<'_, CreatedTab> {
        let espace = request.workspace.to_string();
        if espace.starts_with(PREFIXE_HORS) || espace == ESPACE_INDISPONIBLE {
            return pret(Err(PortError::Unsupported {
                feature: String::from("launching an agent outside a Herdr.app space"),
            }));
        }
        let onglet = nouvel_onglet();
        let labels: BTreeMap<String, String> = [
            (String::from("herdr.space"), espace),
            (String::from("herdr.tab"), onglet.clone()),
        ]
        .into_iter()
        .collect();
        let nom = request.label.as_deref().and_then(nom_de_terminal);
        let resultat = self.verrouiller().and_then(|mut client| {
            match client.terminal_run_shell(&request.cwd, nom.as_deref(), &labels) {
                // Un nom déjà pris n'empêche pas l'onglet : sans nom.
                Err(ErreurTransport::AmontRefus {
                    kind: Some(kind), ..
                }) if kind == "name_taken" => {
                    client.terminal_run_shell(&request.cwd, None, &labels)
                }
                autre => autre,
            }
            .map_err(erreur_port)
        });
        pret(resultat.map(|(adresse, _)| {
            if let Ok(mut lances) = self.lances.lock() {
                lances.insert(
                    onglet.clone(),
                    Lance {
                        adresse: adresse.clone(),
                    },
                );
            }
            tracing::info!(terminal = %adresse, onglet = %onglet, "herdr-core : onglet créé");
            CreatedTab {
                tab: TabId::from(onglet),
                pane: PaneId::from(adresse),
            }
        }))
    }

    /// Tape la commande de l'agent dans le shell d'un onglet créé ici, puis
    /// observe l'amorçage : un statut publié, ou un autre programme au
    /// premier plan.
    ///
    /// - shell sans invite à l'écran → `ShellNotReady` (rien n'a été tapé :
    ///   le moteur peut rejouer) ;
    /// - shell terminé → `Failed` ;
    /// - rien vu dans le délai → `Timeout` : l'agent PEUT exister, le moteur
    ///   ne relance pas et garde l'onglet.
    fn start_agent(&self, request: StartAgent) -> PortFuture<'_, StartEvidence> {
        let adresse = request.pane.to_string();
        if self.lance_de_terminal(&adresse).is_none() {
            return pret(Ok(StartEvidence::Failed {
                detail: format!("terminal {adresse} was not created by Herdr Core Chat"),
            }));
        }
        let ecran = match self
            .verrouiller()
            .and_then(|mut client| client.terminal_read(&adresse, 50).map_err(erreur_port))
        {
            Ok(ecran) => ecran,
            Err(erreur) => return pret(Err(erreur)),
        };
        if ecran.trim().is_empty() {
            return pret(Ok(StartEvidence::ShellNotReady {
                detail: format!("the shell in {adresse} shows no prompt yet"),
            }));
        }
        let avant = match self.terminal(&adresse) {
            Ok(terminal) => terminal,
            Err(erreur) => return pret(Err(erreur)),
        };
        let ligne = ligne_de_lancement(request.kind.as_str(), &request.args);
        if let Err(erreur) = self.verrouiller().and_then(|mut client| {
            client
                .terminal_send(&adresse, &ligne, None, false)
                .map_err(erreur_port)
        }) {
            return pret(Ok(StartEvidence::Failed {
                detail: erreur.description(),
            }));
        }
        tracing::info!(terminal = %adresse, ligne = %ligne, "herdr-core : agent tapé");
        let debut = Instant::now();
        loop {
            let terminal = match self.terminal(&adresse) {
                Ok(terminal) => terminal,
                Err(erreur) => return pret(Err(erreur)),
            };
            if terminal.state == "finished" {
                return pret(Ok(StartEvidence::Failed {
                    detail: format!("the shell in {adresse} exited before `{ligne}` started"),
                }));
            }
            let statut = terminal.status.as_ref().is_some_and(|v| !v.is_null());
            if statut || terminal.foreground_pid != avant.foreground_pid {
                return pret(Ok(StartEvidence::Started {
                    pane: request.pane.clone(),
                }));
            }
            if debut.elapsed() >= self.delai_amorcage {
                return pret(Err(PortError::Timeout));
            }
            std::thread::sleep(PAS_AMORCAGE);
        }
    }

    /// Ferme un onglet **créé ici** (son shell est terminé : Herdr.app ferme
    /// le pane d'un programme sorti). Jamais un onglet de l'utilisateur.
    fn fermer_onglet(&self, tab: TabId) -> PortFuture<'_, ()> {
        let lance = self
            .lances
            .lock()
            .ok()
            .and_then(|mut lances| lances.remove(tab.as_str()));
        let Some(lance) = lance else {
            return pret(Err(PortError::Unsupported {
                feature: format!("closing tab {tab}, which Herdr Core Chat did not create"),
            }));
        };
        pret(
            self.verrouiller()
                .and_then(|mut client| client.terminal_kill(&lance.adresse).map_err(erreur_port)),
        )
    }

    /// L'arrêt : la touche d'arrêt est appuyée (`terminal.interrupt`) —
    /// reçue, pas prouvée exécutée, quel que soit l'`outcome` observé
    /// (`still_working` et `unknown` ne sont PAS « rien à arrêter »).
    fn stop(&self, pane: PaneId) -> PortFuture<'_, StopEvidence> {
        let adresse = pane.to_string();
        let resultat = self.verrouiller().and_then(|mut client| {
            client
                .terminal_interrupt(&adresse)
                .map_err(erreur_port)
                .map(|outcome| {
                    tracing::info!(terminal = %adresse, outcome, "herdr-core : interruption");
                    StopEvidence::InterruptSent
                })
        });
        pret(resultat)
    }
}
