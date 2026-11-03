//! Lecture de `~/.config/herdr-app/workspace-state.json` (chemin injecté
//! par la configuration, `HERDR_CORE_CHAT_HERDR_APP_STATE`).
//!
//! Fichier produit par un **mod** de Herdr.app : il porte les noms calculés
//! par l'app (espaces/onglets). C'est la seule source de ces noms (décision
//! de l'utilisateur, 2026-11-03 : le doc `herdr-app` du core les laisse vides
//! quand ils ne sont pas choisis). On l'utilise pour les **noms** — jamais
//! pour le statut, qui reste celui de l'agent (OSC 7501).
//!
//! ⚠️ Absent → `Ok(None)` (Herdr.app ou son mod ne tourne pas : état
//! normal). Illisible ou de version inconnue → `Err(cause)` : l'appelant la
//! dit (journal) et retombe sur les noms du doc — jamais un nom inventé.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct EtatEspace {
    pub id: String,
    pub name: String,
    pub named: bool,
    pub folder: String,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EtatOnglet {
    pub id: String,
    pub name: String,
    pub named: bool,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EtatAgent {
    pub address: String,
    pub app: Option<String>,
    pub status: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub pinned: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WorkspaceState {
    pub spaces: BTreeMap<String, EtatEspace>,
    pub tabs: BTreeMap<String, EtatOnglet>,
    pub agents: BTreeMap<String, EtatAgent>,
    /// Nom du space pour un terminal donné (via l'entrée `agents`).
    pub terminal_space_name: BTreeMap<String, String>,
    pub terminal_tab_name: BTreeMap<String, String>,
}

impl WorkspaceState {
    /// # Errors
    /// Le fichier existe mais ne se lit pas, n'est pas du JSON, ou porte une
    /// `version` autre que 1 — la cause exacte.
    pub fn charger(chemin: &Path) -> Result<Option<Self>, String> {
        let contenu = match std::fs::read_to_string(chemin) {
            Ok(contenu) => contenu,
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(cause) => return Err(format!("{} : {cause}", chemin.display())),
        };
        let json: Value = serde_json::from_str(&contenu)
            .map_err(|cause| format!("{} : JSON invalide ({cause})", chemin.display()))?;
        match json.get("version").and_then(Value::as_u64) {
            Some(1) => {}
            autre => {
                return Err(format!(
                    "{} : version {autre:?} inconnue (attendue : 1)",
                    chemin.display()
                ));
            }
        }
        let mut etat = WorkspaceState::default();

        if let Some(spaces) = json.get("spaces").and_then(Value::as_array) {
            for sp in spaces {
                let id = sp
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                etat.spaces.insert(
                    id.clone(),
                    EtatEspace {
                        id: id.clone(),
                        name: sp
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        named: sp.get("named").and_then(Value::as_bool).unwrap_or(false),
                        folder: sp
                            .get("folder")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        status: sp.get("status").and_then(Value::as_str).map(String::from),
                    },
                );
                if let Some(tabs) = sp.get("tabs").and_then(Value::as_array) {
                    for tb in tabs {
                        let tid = tb
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if tid.is_empty() {
                            continue;
                        }
                        etat.tabs.insert(
                            tid.clone(),
                            EtatOnglet {
                                id: tid,
                                name: tb
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                named: tb.get("named").and_then(Value::as_bool).unwrap_or(false),
                                status: tb.get("status").and_then(Value::as_str).map(String::from),
                            },
                        );
                    }
                }
            }
        }

        if let Some(agents) = json.get("agents").and_then(Value::as_array) {
            for a in agents {
                let addr = a
                    .get("address")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if addr.is_empty() {
                    continue;
                }
                let space_name = a
                    .get("space")
                    .and_then(|s| s.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let tab_name = a
                    .get("tab")
                    .and_then(|t| t.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !space_name.is_empty() {
                    etat.terminal_space_name.insert(addr.clone(), space_name);
                }
                if !tab_name.is_empty() {
                    etat.terminal_tab_name.insert(addr.clone(), tab_name);
                }
                etat.agents.insert(
                    addr.clone(),
                    EtatAgent {
                        address: addr,
                        app: a.get("app").and_then(Value::as_str).map(String::from),
                        status: a.get("status").and_then(Value::as_str).map(String::from),
                        title: a.get("title").and_then(Value::as_str).map(String::from),
                        cwd: a.get("cwd").and_then(Value::as_str).map(String::from),
                        pinned: a.get("pinned").and_then(Value::as_bool).unwrap_or(false),
                    },
                );
            }
        }

        Ok(Some(etat))
    }
}
