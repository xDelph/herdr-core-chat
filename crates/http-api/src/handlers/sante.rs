//! `GET /api/health` (H01) : la santé du démon depuis le ping coalescé.
//!
//! Herdr injoignable n'est **pas** une erreur du backend : 503 avec le MÊME
//! schéma (`ok:false`, `herdr.reachable:false`) — jamais une 200 qui laisserait
//! croire que tout va bien. Un ping résolu mais illisible est un 500 nommé.

use crate::services::Services;
use application::refresh::ErreurSante;
use axum::response::IntoResponse;
use contracts::http::errors::ApiErrorResponse;
use contracts::http::health::{HealthResponse, HerdrHealth, WatcherHealth};
use contracts::presence::Field;
use domain::errors::{ErreurDomaine, Nature, PortError};
use std::sync::Arc;

/// Le handler — l'uptime vient de l'horloge injectée, jamais du réel.
async fn sante_sur_place(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
) -> axum::response::Response {
    match services.snapshots.sante() {
        Ok(borne) => {
            let ping = borne.ping;
            let reponse = HealthResponse {
                herdr: HerdrHealth {
                    protocol: Field::de_option(Some(ping.protocol)),
                    reachable: true,
                    socket: services.socket.clone(),
                    version: Field::de_option(Some(ping.version)),
                    error: Field(None),
                },
                log: Field::de_option(services.journal_chemin.clone()),
                ok: true,
                uptime_ms: Field::de_option(Some(services.uptime_ms())),
                watcher: Field::de_option(etat_de_veille(&services)),
            };
            (axum::http::StatusCode::OK, axum::Json(reponse)).into_response()
        }
        Err(ErreurSante::Amont { cause }) => match cause {
            // Les seuls cas où le ping ne peut pas raisonnablement « réussir
            // sans valeur » : 500 nommé, jamais un succès fabriqué.
            PortError::Unsupported { .. } | PortError::PathRejected { .. } => {
                // ⚠️ La cause NOMMÉE (« herdr-core protocol generation 2
                // (supported: 1) ») : c'est exactement le cas d'une mise à
                // jour de herdr-core, et « unreadable ping response » la
                // cachait (AGENTS §1.6).
                let erreur = ErreurDomaine::invariant_viole(format!(
                    "unreadable ping response: {}",
                    cause.description()
                ));
                let corps = ApiErrorResponse {
                    cause: Field(None),
                    error: erreur.message_sur(),
                    hint: Field::de_option(Some(format!(
                        "diagnostic code: {}",
                        erreur.diagnostic.code
                    ))),
                    code: Field(None),
                };
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(corps),
                )
                    .into_response()
            }
            // Tout le reste : Herdr ne répond pas — 503 même schéma, AVEC la
            // cause (socket absent, `needs_update`, délai…).
            _ => {
                let reponse = HealthResponse {
                    herdr: HerdrHealth {
                        protocol: Field(None),
                        reachable: false,
                        socket: services.socket.clone(),
                        version: Field(None),
                        error: Field::de_option(Some(cause.description())),
                    },
                    log: Field(None),
                    ok: false,
                    uptime_ms: Field::de_option(Some(services.uptime_ms())),
                    watcher: Field::de_option(etat_de_veille(&services)),
                };
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(reponse),
                )
                    .into_response()
            }
        },
    }
}

/// L'état de la veille, en durées relatives à l'horloge partagée.
fn etat_de_veille(services: &Services) -> Option<WatcherHealth> {
    let boucle = services.veille.get()?;
    let maintenant = services.maintenant.maintenant();
    let depuis = |instant: std::time::Instant| {
        u64::try_from(maintenant.saturating_duration_since(instant).as_millis()).unwrap_or(u64::MAX)
    };
    let derniere = boucle.derniere_erreur();
    Some(WatcherHealth {
        running: boucle.en_marche(),
        failing: boucle.en_panne(),
        last_success_ago_ms: Field::de_option(boucle.dernier_succes().map(depuis)),
        errors: boucle.erreurs(),
        last_error: Field::de_option(derniere.as_ref().map(|(cause, _)| cause.clone())),
        last_error_ago_ms: Field::de_option(derniere.map(|(_, instant)| depuis(instant))),
        events: Field::de_option(Some(boucle.flux_actif())),
        last_event_ago_ms: Field::de_option(boucle.dernier_evenement().map(depuis)),
    })
}

/// L'erreur 500 nommée — réservée à l'invariant interne de santé.
#[must_use]
pub fn reponse_erreur_500(erreur: &ErreurDomaine) -> axum::response::Response {
    debug_assert_eq!(erreur.nature, Nature::InternalInvariant);
    let corps = ApiErrorResponse {
        cause: Field(None),
        error: erreur.message_sur(),
        hint: Field::de_option(Some(format!("diagnostic code: {}", erreur.diagnostic.code))),
        code: Field(None),
    };
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(corps),
    )
        .into_response()
}

/// Le handler `sante`, exécuté hors des workers du runtime ([`crate::bloquant`]).
pub async fn sante(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
) -> axum::response::Response {
    crate::bloquant::hors_runtime(sante_sur_place(axum::extract::State(services))).await
}
