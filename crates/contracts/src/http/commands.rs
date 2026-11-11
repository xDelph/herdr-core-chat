//! Les commandes agents : prompt, arrêt, focus.

use serde::{Deserialize, Serialize};

/// Le corps d'un prompt (`PromptBody`, mesuré — `{ text }`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    pub text: String,
}

/// Un prompt parti (confirmé ou non prouvé — A02 : deux états sur le fil,
/// jamais « envoyé » au hasard).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptSentResponse {
    /// `confirmed | unknown` — la preuve du serveur, ou son absence assumée.
    pub confirmation: crate::events::Confirmation,
    pub fingerprint: String,
    /// `true` : l'agent TRAVAILLAIT à l'envoi — le message est dans sa file,
    /// il le lira à sa prochaine étape ; « confirmé » ne veut pas dire « lu »
    /// (B1.15). Absent : statut non lu.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub queued: crate::presence::Field<bool>,
    /// Le littéral `"sent"`, union à un membre : sa présence est vérifiée.
    pub status: crate::events::PromptStatus,
}

/// Un prompt supprimé — double envoi assumé, pas une erreur (D007.2).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptSuppressedResponse {
    pub reason: String,
    pub status: SuppressedStatus,
}

/// Le littéral `"suppressed"` — union à un membre, vérifiée au décodage.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SuppressedStatus {
    Suppressed,
}

/// Un prompt qui n'est pas parti — l'état réel, nommé, avec son détail.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptFailedResponse {
    pub detail: String,
    /// `attente | erreur | inconnu` — B05 : trois états, jamais « envoyé ».
    pub state: SendState,
    /// Le code stable de l'échec (voir `ApiErrorResponse.code`).
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub code: crate::presence::Field<String>,
}

/// L'union fermée des états d'un envoi non parti (libellés du fil exacts).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SendState {
    Attente,
    Erreur,
    Inconnu,
}

/// Épingler ou désépingler un agent (H25) — partagé avec Herdr.app.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinRequest {
    pub pinned: bool,
}

/// L'épingle posée (H25).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinResponse {
    pub pane_id: String,
    pub pinned: bool,
}

/// Une pièce jointe déposée à côté du terminal (H26) : rien n'a été tapé,
/// `path` est à coller dans le message.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentResponse {
    /// Où le fichier a atterri, vu de la machine du terminal.
    pub path: String,
    /// Le nom gardé (assaini).
    pub name: String,
    pub bytes: u64,
}

/// Une interruption demandée (`StopAgentResponse`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StopAgentResponse {
    /// `interrupt-sent | not-running` — reçu, pas prouvé exécuté.
    pub status: StopStatus,
    /// Ce que herdr-core a vu après la touche (`stopped | exited |
    /// still-working | unknown`) — absent quand il n'a rien observé.
    #[serde(default, skip_serializing_if = "crate::presence::Field::est_absente")]
    pub outcome: crate::presence::Field<domain::ports::StopOutcome>,
}

/// L'union fermée des résultats d'arrêt.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StopStatus {
    InterruptSent,
    NotRunning,
}

/// Un focus appliqué (`AgentFocusedResponse`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentFocusedResponse {
    pub focused: bool,
    /// Le littéral `"agent-focused"`, vérifié au décodage.
    pub status: FocusStatus,
}

/// Le littéral `"agent-focused"` — union à un membre.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FocusStatus {
    AgentFocused,
}
