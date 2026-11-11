//! `GET /api/overview` et la famille snapshot passée au client.

use crate::presence::Nullable;
use domain::status::AgentStatus;
use serde::{Deserialize, Serialize};

/// Une ligne d'agent de l'overview (homologue exact de `AgentViewSchema`) :
/// **tous les champs sont présents** — les vagues d'information passent par
/// `Nullable` (null mesuré), jamais par une absence.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub agent: Nullable<String>,
    pub cwd: Nullable<String>,
    pub focused: bool,
    pub pane_id: String,
    pub parent_session: Nullable<String>,
    pub session_cwd: Nullable<String>,
    /// Trois états sur le fil : `true | false | null` — null = non
    /// vérifiable (D03), jamais confondu avec faux.
    pub session_cwd_matches: Nullable<bool>,
    pub session_id: Nullable<String>,
    pub session_kind: Nullable<crate::SessionRefKind>,
    pub session_path: Nullable<String>,
    pub status: AgentStatus,
    pub tab_label: Nullable<String>,
    pub tab_id: String,
    pub tab_number: Nullable<u64>,
    pub name: Nullable<String>,
    pub pane_rank: Nullable<u64>,
    pub tab_pane_count: u64,
    pub title: Nullable<String>,
    pub workspace_label: Nullable<String>,
    pub workspace_id: String,
    /// Le tour de herdr-core (`turn`) — il avance à chaque sortie de
    /// `working` (B1.4).
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub turn: crate::presence::Field<u64>,
    /// Ce qu'un agent `blocked` attend : `permission | question | auth` —
    /// absent quand il ne le dit pas.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub blocked_kind: crate::presence::Field<crate::BlockedKind>,
    /// La fin du terminal (code, raison, signal) quand herdr-core le dit fini.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub exit: crate::presence::Field<AgentExit>,
    /// Épinglé, sur le Mac (Herdr.app) ou depuis le téléphone — présent
    /// seulement quand vrai.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub pinned: crate::presence::Field<bool>,
    /// Les sous-agents en direct (records 7501) — absent quand il n'y en a pas.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub subagents: crate::presence::Field<Vec<Subagent>>,
    /// L'écran peut être incomplet : herdr-core a crashé (`screen: inexact`).
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub screen_inexact: crate::presence::Field<bool>,
}

/// Un sous-agent en direct : un record 7501 du programme, ses mots à lui.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Subagent {
    pub id: String,
    pub status: AgentStatus,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub title: crate::presence::Field<String>,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub msg: crate::presence::Field<String>,
}

impl From<&domain::status::SousAgent> for Subagent {
    fn from(sous: &domain::status::SousAgent) -> Self {
        Self {
            id: sous.id.clone(),
            status: sous.status,
            title: crate::presence::Field::de_option(sous.title.clone()),
            msg: crate::presence::Field::de_option(sous.msg.clone()),
        }
    }
}

/// Les trois champs du lot 4, tirés des faits du terminal — chacun absent
/// quand il n'a rien à dire (le corpus figé reste valide).
#[must_use]
pub fn champs_des_faits(
    faits: &domain::status::FaitsDuTerminal,
) -> (
    crate::presence::Field<bool>,
    crate::presence::Field<Vec<Subagent>>,
    crate::presence::Field<bool>,
) {
    (
        crate::presence::Field::de_option(faits.epingle.then_some(true)),
        crate::presence::Field::de_option(
            (!faits.sous_agents.is_empty())
                .then(|| faits.sous_agents.iter().map(Into::into).collect()),
        ),
        crate::presence::Field::de_option(faits.ecran_inexact.then_some(true)),
    )
}

/// La fin d'un terminal, telle que herdr-core la dit (`exit_code`,
/// `reason`, `signal`) — chaque champ absent reste absent.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentExit {
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub code: crate::presence::Field<i64>,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub reason: crate::presence::Field<String>,
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub signal: crate::presence::Field<String>,
}

impl From<&domain::status::AgentExit> for AgentExit {
    fn from(fin: &domain::status::AgentExit) -> Self {
        Self {
            code: crate::presence::Field::de_option(fin.code),
            reason: crate::presence::Field::de_option(fin.reason.clone()),
            signal: crate::presence::Field::de_option(fin.signal.clone()),
        }
    }
}

/// Une ligne de workspace de l'overview (homologue de `WorkspaceViewSchema`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub agent_panes: Vec<String>,
    pub agent_status: Nullable<AgentStatus>,
    /// Le chemin réel où lancer un agent — absent plutôt qu'inventé (D02).
    pub checkout_path: Nullable<String>,
    pub focused: bool,
    pub label: Nullable<String>,
    pub pane_count: u64,
    pub tab_count: u64,
    pub workspace_id: String,
}

/// La réponse d'overview complète.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverviewResponse {
    pub agents: Vec<AgentView>,
    pub herdr_protocol: u64,
    pub herdr_version: String,
    pub without_session: u64,
    pub workspaces: Vec<WorkspaceView>,
}
