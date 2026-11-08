//! `POST /api/agents/:paneId/model` (H24) : changer le modèle et/ou la
//! réflexion d'un agent **en session**.
//!
//! ⚠️ **Le type d'agent vient de Herdr, jamais du client** : la recette tape
//! des commandes et des touches, et celles d'un agent ne veulent rien dire —
//! ou pire — dans un autre. Le pane est relu (`list_agents`) avant toute
//! frappe ; sans agent, 404 et rien n'est tapé.
//!
//! Les recettes et leurs gardes vivent dans `interactions::reglage` ; un refus
//! est une **valeur** (`refusee`), pas une erreur du fil.

use crate::corps::CorpsBorne;
use crate::services::Services;
use application::commands::{DemandeReglage, EtatReglage, regle_modele};
use axum::response::IntoResponse;
use contracts::http::interactive::{
    AnswerState, ModelChangeRequest, ModelChangedResponse, ModelChangedStatus,
};
use contracts::registry::RouteId;
use domain::errors::{ErreurDomaine, PortError};
use domain::ids::PaneId;
use std::sync::Arc;

fn poll_pret<T>(mut fut: domain::ports::PortFuture<'_, T>) -> Result<T, PortError> {
    let mut contexte = std::task::Context::from_waker(std::task::Waker::noop());
    match fut.as_mut().poll(&mut contexte) {
        std::task::Poll::Ready(reponse) => reponse,
        std::task::Poll::Pending => Err(PortError::Transport {
            detail: String::from("the Herdr port did not answer synchronously"),
        }),
    }
}

async fn reglage_sur_place(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    let Ok(corps) = serde_json::from_slice::<ModelChangeRequest>(&corps.octets) else {
        return crate::handlers::repondre_erreur(
            RouteId::ReglageModele,
            &ErreurDomaine::entree_invalide(
                "invalid body: `model` and/or `thinking` (strings) expected",
            ),
        );
    };
    let cible = PaneId::from(pane_id);
    let agents = match poll_pret(services.port.list_agents()) {
        Ok(agents) => agents,
        Err(cause) => {
            return crate::handlers::repondre_erreur(
                RouteId::ReglageModele,
                &ErreurDomaine::amont_injoignable(format!(
                    "Herdr agent list unavailable: {}",
                    cause.description()
                )),
            );
        }
    };
    let Some(kind) = agents
        .agents
        .iter()
        .find(|agent| agent.pane_id == cible)
        .and_then(|agent| agent.agent.as_ref())
        .map(|kind| kind.as_str().to_string())
    else {
        return crate::handlers::repondre_erreur(
            RouteId::ReglageModele,
            &ErreurDomaine::sans_source("no known agent in this pane — nothing was typed"),
        );
    };
    let modele = corps.model.en_option();
    let reflexion = corps.thinking.en_option();
    let demande = DemandeReglage {
        kind: &kind,
        modele: modele.as_deref(),
        reflexion: reflexion.as_deref(),
    };
    match regle_modele(services.port.as_ref(), &cible, demande) {
        Ok(reglage) => (
            axum::http::StatusCode::OK,
            axum::Json(ModelChangedResponse {
                detail: reglage.detail,
                state: etat_de_fil(reglage.etat),
                status: ModelChangedStatus::ModelChanged,
            }),
        )
            .into_response(),
        // ⚠️ Un délai sans verdict n'est PAS un refus : des frappes ont pu
        // partir, un sélecteur peut être resté ouvert.
        Err(PortError::Timeout) => (
            axum::http::StatusCode::OK,
            axum::Json(ModelChangedResponse {
                detail: String::from(
                    "Herdr did not answer in time: keys may have been typed — check the screen \
                     before trying again",
                ),
                state: AnswerState::Transmise,
                status: ModelChangedStatus::ModelChanged,
            }),
        )
            .into_response(),
        Err(cause) => crate::handlers::repondre_erreur(
            RouteId::ReglageModele,
            &ErreurDomaine::amont_refus(format!("model change failed: {}", cause.description())),
        ),
    }
}

/// Les trois états du fil.
#[must_use]
pub fn etat_de_fil(etat: EtatReglage) -> AnswerState {
    match etat {
        EtatReglage::Applique => AnswerState::Appliquee,
        EtatReglage::Transmis => AnswerState::Transmise,
        EtatReglage::Refuse => AnswerState::Refusee,
    }
}

/// Le handler `reglage`, exécuté hors des workers du runtime ([`crate::bloquant`]).
pub async fn reglage(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    corps: CorpsBorne,
) -> axum::response::Response {
    crate::bloquant::hors_runtime(reglage_sur_place(
        axum::extract::State(services),
        axum::extract::Path(pane_id),
        corps,
    ))
    .await
}
