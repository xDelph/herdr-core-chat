//! `POST /api/agents/:paneId/answer` (H10) : répondre à la question affichée.
//!
//! ⚠️ **On n'envoie jamais une touche qui n'est pas un choix offert par la
//! question affichée** — pas de « 1 par défaut », pas d'`enter` seul. Une
//! invite peut autoriser l'exécution de code : envoyer un chiffre que
//! personne n'a proposé serait envoyer une décision que l'utilisateur n'a
//! pas prise. Le numéro est validé contre les choix **réellement affichés**,
//! dans le service (`interactions::repondre`), pas ici.
//!
//! ⚠️ Un refus est une **valeur** (`refusee`), pas une erreur du fil : rien
//! n'a été envoyé, et la cause est nommée.

use crate::corps::CorpsBorne;
use crate::services::Services;
use application::commands::{EtatReponse, repondre_question};
use axum::response::IntoResponse;
use contracts::http::interactive::{AnswerState, AnsweredStatus, QuestionAnsweredResponse};
use contracts::registry::RouteId;
use domain::errors::ErreurDomaine;
use domain::ids::PaneId;
use std::sync::Arc;

/// Le corps exact du fil : `{ numero: number, empreinte: string, texte?: string }`.
///
/// ⚠️ **L'empreinte est requise** (B1.1) : c'est celle de la question que le
/// téléphone a AFFICHÉE. Sans elle, un numéro validait n'importe quelle
/// question arrivée entre-temps.
#[derive(serde::Deserialize)]
struct Corps {
    numero: i64,
    empreinte: String,
    #[serde(default)]
    texte: Option<String>,
}

/// La réponse à une question.
async fn reponse_sur_place(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    let Ok(corps) = serde_json::from_slice::<Corps>(&corps.octets) else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "invalid body: `numero` (integer) and `empreinte` (string) required — reload the app"
            })),
        )
            .into_response();
    };
    let cible = PaneId::from(pane_id);
    match repondre_question(
        services.port.as_ref(),
        &cible,
        corps.numero,
        corps.texte.as_deref(),
        Some(&corps.empreinte),
    ) {
        // La question a changé : 409, rien n'est parti. Le corps garde la
        // forme du contrat (`refusee`) — le client affiche la cause.
        Ok(reponse) if reponse.etat == EtatReponse::Perimee => (
            axum::http::StatusCode::CONFLICT,
            axum::Json(QuestionAnsweredResponse {
                detail: reponse.detail,
                state: AnswerState::Refusee,
                status: AnsweredStatus::QuestionAnswered,
                code: contracts::presence::Field::de_option(Some(String::from("screen_changed"))),
            }),
        )
            .into_response(),
        Ok(reponse) => (
            axum::http::StatusCode::OK,
            axum::Json(QuestionAnsweredResponse {
                detail: reponse.detail,
                state: etat_de_fil(reponse.etat),
                status: AnsweredStatus::QuestionAnswered,
                code: contracts::presence::Field::de_option(None),
            }),
        )
            .into_response(),
        // ⚠️ Un délai sans verdict n'est PAS un refus : des touches ont pu
        // partir. « Answer rejected » faisait répondre une seconde fois à
        // une question peut-être déjà validée.
        Err(domain::errors::PortError::Timeout) => (
            axum::http::StatusCode::OK,
            axum::Json(QuestionAnsweredResponse {
                detail: String::from(
                    "Herdr did not answer in time: the answer may have been applied — \
                     check the screen before answering again",
                ),
                state: AnswerState::Transmise,
                status: AnsweredStatus::QuestionAnswered,
                code: contracts::presence::Field::de_option(Some(String::from("timeout"))),
            }),
        )
            .into_response(),
        // L'échec Herdr est un 502 : le corps porte la cause réelle.
        Err(cause) => crate::handlers::repondre_erreur(
            RouteId::ReponseQuestion,
            &ErreurDomaine::amont_refus(format!("answer failed: {}", cause.description()))
                .avec_port(&cause),
        ),
    }
}

/// Les trois états du fil — `appliquee | transmise | refusee`.
#[must_use]
pub fn etat_de_fil(etat: EtatReponse) -> AnswerState {
    match etat {
        EtatReponse::Appliquee => AnswerState::Appliquee,
        EtatReponse::Transmise => AnswerState::Transmise,
        EtatReponse::Refusee | EtatReponse::Perimee => AnswerState::Refusee,
    }
}

/// Le handler `reponse`, exécuté hors des workers du runtime ([`crate::bloquant`]).
pub async fn reponse(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    crate::bloquant::hors_runtime(reponse_sur_place(
        axum::extract::State(services),
        axum::extract::Path(pane_id),
        corps,
    ))
    .await
}
