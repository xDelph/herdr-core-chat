//! `POST /api/agents/:paneId/prompt` (H05) : l'envoi d'un texte, avec la
//! déduplication **après preuve** et l'état B05 nommé (`attente | erreur |
//! inconnu`) — jamais « envoyé » sans confirmation.
//!
//! ⚠️ **Un agent bloqué attend des FRAPPES, pas un prompt.** Herdr refuse
//! lui-même `agent.prompt` dans ce cas (« agent … is blocked and requires
//! interactive input », mesuré le 2026-10-16). Sans ce branchement, « type
//! your own answer » était **impossible** : le seul chemin d'écriture
//! existant était précisément celui que Herdr refuse.
//!
//! ⚠️ Le statut est relu **ici, depuis Herdr** (`lecture_forcee`), jamais
//! reçu du client : un client périmé enverrait le mauvais mode, et c'est
//! Herdr qui décide de ce qu'il accepte. Une lecture de statut qui échoue ne
//! bloque pas l'envoi : elle laisse `saisie = false`, comme le fil.

use crate::corps::{CorpsBorne, verifie_prompt};
use crate::services::Services;
use application::commands::prompt::{Confirmation as ConfirmationEnvoi, RefusEnvoi};
use application::refresh::ErreurLecture;
use axum::response::IntoResponse;
use contracts::events::{Confirmation, PromptStatus};
use contracts::http::commands::{
    PromptFailedResponse, PromptSentResponse, PromptSuppressedResponse, SendState, SuppressedStatus,
};
use contracts::presence::Field;
use domain::errors::PortError;
use domain::ids::PaneId;
use domain::status::AgentStatus;
use std::sync::Arc;

/// Le corps exact du fil : `{ text: string }` — un texte vide reste un
/// texte (l'agent décide), un texte absent est une erreur.
#[derive(serde::Deserialize)]
struct Corps {
    text: String,
}

/// L'envoi du prompt.
async fn prompt_sur_place(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    // 1) Le corps — malformé = 400 nommé, jamais un envoi à moitié décidé.
    let Ok(corps) = serde_json::from_slice::<Corps>(&corps.octets) else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "invalid body: `text` (string) required"
            })),
        )
            .into_response();
    };
    // 2) La taille du TEXTE en octets UTF-8 (S06) — refusée AVANT toute
    //    action Herdr : un prompt tronqué après coup laisserait l'agent dans
    //    un état incompréhensible.
    if let Err(refus) = verifie_prompt(&corps.text) {
        return *refus;
    }
    let cible = PaneId::from(pane_id);
    // 3) Le mode : un agent bloqué reçoit des frappes. La lecture est
    //    FORCÉE (hors fenêtre de cache) — mais son échec ne bloque pas :
    //    le mode retombe sur « prompt littéral », comme le fil.
    let lu = match services.snapshots.lecture_forcee() {
        Ok(borne) => borne
            .snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == cible)
            .map(|agent| (agent.status, agent.revision)),
        Err(ErreurLecture::Amont { .. }) => None,
    };
    let statut = lu.map(|(statut, _)| statut);
    let saisie = statut == Some(AgentStatus::Blocked);
    // 4) L'envoi — la déduplication (au même tour) et la preuve vivent dans
    //    le service.
    match services.prompt.envoie_au_tour(
        services.port.as_ref(),
        &cible,
        &corps.text,
        saisie,
        lu.map(|(_, tour)| tour),
    ) {
        Ok(envoi) => {
            // Le direct : l'envoi est publié AVEC sa preuve (A02), jamais
            // une intention. Un journal plein ne fait pas échouer l'envoi
            // déjà effectué — la trace est un confort, pas la preuve.
            let confirme = envoi.confirmation == ConfirmationEnvoi::Confirme;
            let _ = crate::handlers::events::publie(
                services.journal.as_ref(),
                &[crate::publication::evenement_de_prompt(
                    cible.as_str(),
                    &envoi.empreinte,
                    confirme,
                )],
            );
            (
                axum::http::StatusCode::OK,
                axum::Json(PromptSentResponse {
                    confirmation: match envoi.confirmation {
                        ConfirmationEnvoi::Confirme => Confirmation::Confirmed,
                        ConfirmationEnvoi::Inconnu => Confirmation::Unknown,
                    },
                    fingerprint: envoi.empreinte,
                    // Lu juste avant l'envoi (lecture forcée) : un agent au
                    // travail met le message en file (B1.15).
                    queued: contracts::presence::Field::de_option(
                        statut.map(|statut| statut == AgentStatus::Working),
                    ),
                    status: PromptStatus::Sent,
                }),
            )
                .into_response()
        }
        // Un double envoi n'est pas une erreur serveur : un no-op assumé.
        Err(RefusEnvoi::Doublon { .. }) => {
            // Le no-op assumé est publié aussi : l'interface qui écoute le
            // direct ne doit pas croire à un envoi.
            let _ = crate::handlers::events::publie(
                services.journal.as_ref(),
                &[crate::publication::evenement_de_prompt_supprime(
                    cible.as_str(),
                )],
            );
            (
                axum::http::StatusCode::OK,
                axum::Json(PromptSuppressedResponse {
                    reason: String::from("identical send already confirmed within the window"),
                    status: SuppressedStatus::Suppressed,
                }),
            )
                .into_response()
        }
        // La file du pane est pleine : RIEN n'est parti — un refus nommé
        // (`erreur`), le client peut réessayer. Jamais « envoyé ».
        // 429 + `busy` : le client propose « Retry ».
        Err(RefusEnvoi::Sature { detail, .. }) => (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            axum::Json(PromptFailedResponse {
                detail,
                state: SendState::Erreur,
                code: Field::de_option(Some(String::from("busy"))),
            }),
        )
            .into_response(),
        // Une question ou une confiance à l'écran : RIEN n'est parti (B1.2).
        // 409 — l'état de l'agent, pas une panne : on répond par la carte.
        Err(RefusEnvoi::InviteOuverte { detail, .. }) => (
            axum::http::StatusCode::CONFLICT,
            axum::Json(PromptFailedResponse {
                detail,
                state: SendState::Erreur,
                code: Field::de_option(Some(String::from("prompt_open"))),
            }),
        )
            .into_response(),
        // B05 : l'état vient de la NATURE de la panne, jamais d'une phrase ;
        // le statut et le code, du `data.kind` de herdr-core (agent parti
        // 404, terminal fini 410, bloqué 409…), sinon 502.
        Err(RefusEnvoi::NonConfirme { detail, cause, .. }) => {
            let amont = match &cause {
                PortError::Upstream { code, .. } => crate::errors::code_stable(code),
                PortError::Timeout => crate::errors::code_stable("timeout"),
                _ => None,
            };
            let statut = amont
                .and_then(|(statut, _)| axum::http::StatusCode::from_u16(statut).ok())
                .unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
            (
                statut,
                axum::Json(PromptFailedResponse {
                    detail,
                    state: etat_de_panne(&cause),
                    code: Field::de_option(amont.map(|(_, code)| String::from(code))),
                }),
            )
                .into_response()
        }
    }
}

/// L'état B05 d'une panne de transport — la nature décide :
/// un délai n'a rien prouvé (`attente`), un transport muet non plus
/// (`inconnu`), un refus amont est un envoi perdu (`erreur`).
#[must_use]
pub fn etat_de_panne(cause: &PortError) -> SendState {
    match cause {
        PortError::Timeout => SendState::Attente,
        PortError::Transport { .. } => SendState::Inconnu,
        PortError::Upstream { .. } | PortError::Unsupported { .. } => SendState::Erreur,
        PortError::PathRejected { .. } => SendState::Erreur,
    }
}

/// Le handler `prompt`, exécuté hors des workers du runtime ([`crate::bloquant`]).
pub async fn prompt(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    crate::bloquant::hors_runtime(prompt_sur_place(
        axum::extract::State(services),
        axum::extract::Path(pane_id),
        corps,
    ))
    .await
}
