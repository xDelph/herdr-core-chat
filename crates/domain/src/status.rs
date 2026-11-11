//! États à **union fermée**.
//!
//! ⚠️ **Pas de chaîne libre pour un état métier** (A04) : chaque nom vient du
//! schéma Herdr vérifié par exécution (protocole 22), et un état inconnu est un
//! refus de décodage, pas un étiquetage au hasard. Une union fermée qui
//! grossit casse la compilation des consommateurs tant que le nouveau cas
//! n'est pas traité — exactement ce qu'on veut.

use serde::{Deserialize, Serialize};
use std::fmt;

/// État d'un agent : `idle | working | blocked | done | error | unknown`.
///
/// Les cinq premiers sont les mots du programme (OSC 7501, `Status.state` de
/// herdr-core 0.1.17 : « idle, working, done, blocked or error »).
/// ⚠️ `unknown` n'est PAS un mot de herdr-core : c'est l'agent reconnu dont
/// le programme ne dit rien (pas de statut, ou un mot hors de cette liste).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    /// Le programme s'est déclaré en erreur (`error`, herdr-core).
    Error,
    Unknown,
}

/// Ce qu'un agent `blocked` attend (`status.kind` de herdr-core) :
/// `permission | question | auth`. Un mot hors de la liste, ou aucun mot :
/// pas de nature — jamais rapprochée d'une voisine.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BlockedKind {
    /// Une autorisation d'outil (« Allow this Bash command? »).
    Permission,
    /// Une question posée à l'utilisateur.
    Question,
    /// Une connexion à terminer (sign-in).
    Auth,
}

impl BlockedKind {
    /// Le mot de herdr-core → la nature ; inconnu : `None`.
    #[must_use]
    pub fn depuis_mot(mot: &str) -> Option<Self> {
        match mot {
            "permission" => Some(Self::Permission),
            "question" => Some(Self::Question),
            "auth" => Some(Self::Auth),
            _ => None,
        }
    }
}

/// La fin d'un terminal (`state: finished` chez herdr-core) : ce que le core
/// en dit, tel quel — code de sortie, raison, signal. Chaque champ absent
/// reste absent : rien n'est déduit.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentExit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}

impl AgentExit {
    /// Une fin en échec : un code non nul, ou un signal.
    #[must_use]
    pub fn en_echec(&self) -> bool {
        self.code.is_some_and(|code| code != 0) || self.signal.is_some()
    }

    /// ⚠️ **La fin expliquée (lot 4)**, en mots d'utilisateur, après le nom
    /// de l'agent : « was killed », « exited with code 1 ». Les raisons sont
    /// celles du schéma herdr-core 0.1.19 (`reason`) ; un mot inconnu est
    /// montré tel quel, jamais traduit au hasard.
    #[must_use]
    pub fn explication(&self) -> String {
        let code = self
            .code
            .map(|code| format!(" (code {code})"))
            .unwrap_or_default();
        match self.reason.as_deref() {
            Some("killed") => format!("was killed{code}"),
            Some("signaled") => match &self.signal {
                Some(signal) => format!("died from signal {signal}"),
                None => String::from("died from a signal"),
            },
            Some("server_stop") => format!("stopped when herdr-core stopped{code}"),
            Some("host_shutdown") => format!("stopped when the Mac shut down{code}"),
            Some("holder_lost") => String::from("was lost: the process holding it died"),
            Some("stopped_by_restart") => {
                String::from("was not running any more after herdr-core restarted")
            }
            Some("spawn_pending") => String::from("is still starting"),
            Some("exited") | None => {
                let mut fin = match self.code {
                    Some(code) => format!("exited with code {code}"),
                    None => String::from("exited"),
                };
                if let Some(signal) = &self.signal {
                    fin.push_str(&format!(" (signal {signal})"));
                }
                fin
            }
            Some(autre) => format!("ended ({autre}){code}"),
        }
    }

    /// Perdu plutôt que fini : le processus qui le portait est mort.
    #[must_use]
    pub fn perdu(&self) -> bool {
        self.reason.as_deref() == Some("holder_lost")
    }
}

/// Un sous-agent en direct : un **record 7501** du programme (`records` de
/// herdr-core, un par id) — Claude Code en porte un par sous-agent lancé.
/// Les mots sont ceux du programme : rien n'est traduit ni deviné.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SousAgent {
    /// L'identifiant du record, tel que le programme le donne.
    pub id: String,
    pub status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

/// Les faits du core qui ne sont ni le statut ni la fin (lot 4) : les
/// sous-agents en direct, l'épingle partagée avec Herdr.app, l'écran
/// inexact après un crash du core. Tous vides par défaut.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct FaitsDuTerminal {
    /// Les records 7501 du programme, dans l'ordre de leurs ids.
    pub sous_agents: Vec<SousAgent>,
    /// Épinglé (`herdr.pinned`, posé par Herdr.app ou par nous).
    pub epingle: bool,
    /// `screen: inexact` : le core a crashé, l'historique peut manquer.
    pub ecran_inexact: bool,
}

/// État de la conversation visible sur une ligne de conversation
/// (`available | absent | error`, B10).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Available,
    Absent,
    Error,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let texte = match self {
            AgentStatus::Idle => "idle",
            AgentStatus::Working => "working",
            AgentStatus::Blocked => "blocked",
            AgentStatus::Done => "done",
            AgentStatus::Error => "error",
            AgentStatus::Unknown => "unknown",
        };
        f.write_str(texte)
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let texte = match self {
            SessionState::Available => "available",
            SessionState::Absent => "absent",
            SessionState::Error => "error",
        };
        f.write_str(texte)
    }
}

/// Reconnaît le libellé exact d'un [`AgentStatus`], sinon rien.
///
/// Le décodage **filaire** appartient à `herdr-client` (T021) ; le domaine
/// offre la fonction totale, utile aux politiques et aux tests.
#[must_use]
pub fn statut_d_agent(raw: &str) -> Option<AgentStatus> {
    match raw {
        "idle" => Some(AgentStatus::Idle),
        "working" => Some(AgentStatus::Working),
        "blocked" => Some(AgentStatus::Blocked),
        "done" => Some(AgentStatus::Done),
        "error" => Some(AgentStatus::Error),
        "unknown" => Some(AgentStatus::Unknown),
        _ => None,
    }
}

/// Reconnaît le libellé exact d'un [`SessionState`], sinon rien.
#[must_use]
pub fn etat_de_session(raw: &str) -> Option<SessionState> {
    match raw {
        "available" => Some(SessionState::Available),
        "absent" => Some(SessionState::Absent),
        "error" => Some(SessionState::Error),
        _ => None,
    }
}
