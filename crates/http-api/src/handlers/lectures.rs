//! `POST /api/agents/:paneId/read` (H22) : noter qu'une conversation a été
//! lue — pour TOUS les appareils (`application::lectures`).
//!
//! ⚠️ Lire ne vole jamais le focus du Mac. Mais le « vu » est partagé avec
//! Herdr.app (`herdr.seen`) : un `done` lu ici s'éteint aussi là-bas — sinon
//! le Mac redemandait l'attention pour une réponse déjà lue sur le téléphone.
//! ⚠️ Une panne de persistance n'est pas masquée : elle part au journal
//! (la lecture vaut jusqu'au redémarrage, la réponse dit l'instant retenu).

use crate::corps::CorpsBorne;
use crate::services::Services;
use axum::response::IntoResponse;
use contracts::http::lectures::{MarkReadRequest, MarkReadResponse};
use std::sync::Arc;

/// La lecture notée — l'instant retenu ne recule jamais.
async fn lecture_sur_place(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    let Ok(demande) = serde_json::from_slice::<MarkReadRequest>(&corps.octets) else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "invalid body: `at` (milliseconds, integer) required"
            })),
        )
            .into_response();
    };
    let notee = services.lectures.note(&pane_id, demande.at);
    if let Some(panne) = &notee.panne {
        tracing::warn!(pane = %pane_id, cause = %panne, "lecture partagée non persistée");
    }
    // Le « vu » de Herdr.app : un échec n'annule pas la lecture notée ici,
    // mais sa cause part au journal (jamais masquée).
    //
    // ⚠️ **L'instant vu se compare à l'horloge du Mac** (`status_ms`) : un
    // téléphone en retard de quelques secondes ne partageait jamais son
    // « vu ». La PWA poste la lecture à l'instant où l'écran la montre — la
    // réception ici EST cet instant, à l'horloge du Mac.
    let vu_a = notee.lue_a.max(maintenant_ms());
    match application::lectures::partage_vu(services.port.as_ref(), &pane_id, vu_a) {
        Ok(Some(tour)) => {
            tracing::info!(pane = %pane_id, tour, "vu partagé avec Herdr.app (herdr.seen)");
        }
        Ok(None) => {}
        Err(cause) => {
            tracing::warn!(pane = %pane_id, cause = %cause.description(), "vu non partagé avec Herdr.app");
        }
    }
    (
        axum::http::StatusCode::OK,
        axum::Json(MarkReadResponse {
            pane_id,
            read_at: notee.lue_a,
        }),
    )
        .into_response()
}

/// L'instant présent, en millisecondes UTC (horloge du Mac).
fn maintenant_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duree| {
            u64::try_from(duree.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Le handler `lecture`, exécuté hors des workers du runtime
/// ([`crate::bloquant`]) : le « vu » partagé passe par le socket du core.
pub async fn lecture(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    crate::bloquant::hors_runtime(lecture_sur_place(
        axum::extract::State(services),
        axum::extract::Path(pane_id),
        corps,
    ))
    .await
}
