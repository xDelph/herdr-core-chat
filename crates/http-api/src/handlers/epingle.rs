//! `POST /api/agents/:paneId/pin` (H25) : épingler un agent en tête des
//! listes — **pour tous les clients** : sous herdr-core, le label
//! `herdr.pinned` que Herdr.app pose et lit lui-même (`terminal.pin`).
//!
//! ⚠️ Le format appartient à Herdr.app : `"1"` épingle, une valeur vide
//! retire (lu dans Herdr.app 0.1.13). Rien d'autre n'est écrit.

use crate::corps::CorpsBorne;
use crate::services::Services;
use axum::response::IntoResponse;
use contracts::http::commands::{PinRequest, PinResponse};
use contracts::registry::RouteId;
use domain::errors::ErreurDomaine;
use domain::ids::PaneId;
use std::sync::Arc;

async fn epingle_sur_place(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    let Ok(demande) = serde_json::from_slice::<PinRequest>(&corps.octets) else {
        return crate::handlers::repondre_erreur(
            RouteId::Epingle,
            &ErreurDomaine::entree_invalide("invalid body: `pinned` (boolean) required"),
        );
    };
    let cible = PaneId::from(pane_id.clone());
    let mut futur = services.port.epingle(cible, demande.pinned);
    match application::commands::attend_port(&mut futur) {
        Ok(()) => {
            tracing::info!(pane = %pane_id, epingle = demande.pinned, "épingle partagée avec Herdr.app");
            // La liste doit le montrer sans attendre la prochaine veille.
            if let Some(veille) = services.veille.get() {
                veille.reveille();
            }
            (
                axum::http::StatusCode::OK,
                axum::Json(PinResponse {
                    pane_id,
                    pinned: demande.pinned,
                }),
            )
                .into_response()
        }
        Err(cause) => crate::handlers::repondre_erreur(
            RouteId::Epingle,
            &ErreurDomaine::amont_refus(format!("pin failed: {}", cause.description()))
                .avec_port(&cause),
        ),
    }
}

/// Le handler `epingle`, exécuté hors des workers du runtime ([`crate::bloquant`]).
pub async fn epingle(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    crate::bloquant::hors_runtime(epingle_sur_place(
        axum::extract::State(services),
        axum::extract::Path(pane_id),
        corps,
    ))
    .await
}
