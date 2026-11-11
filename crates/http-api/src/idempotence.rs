//! L'idempotence des mutations : l'en-tête `Idempotency-Key` (F1.10/F1.11).
//!
//! ⚠️ **Une action à l'écran ne se rejoue pas.** Un POST répété par le
//! transport (réseau mobile qui bascule, proxy qui réessaie, onglet réveillé)
//! retapait une réponse, relançait un agent, renvoyait un prompt. Avec la
//! même clé :
//!
//! - la première requête s'exécute, sa réponse est gardée ;
//! - une répétition pendant qu'elle tourne reçoit **409** — rien n'est refait ;
//! - une répétition après reçoit la **même réponse**, rejouée telle quelle
//!   (`Idempotent-Replayed: true`), sans toucher Herdr.
//!
//! ⚠️ **Le cache est borné** (256 clés, 10 min) : une clé oubliée redevient
//! une requête neuve, ce qui reste le comportement sans en-tête. Une requête
//! sans clé n'est pas touchée (`curl`, vieux clients).
//!
//! ⚠️ **Une entrée « en cours » ne survit pas à son handler** : la garde RAII
//! la retire si la requête est abandonnée ou si le handler panique — sans
//! elle, la clé resterait bloquée en 409 pour toujours.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Le nombre maximal de clés retenues.
pub const CLES_MAX: usize = 256;
/// La durée de vie d'une réponse retenue.
pub const DUREE_DE_VIE: Duration = Duration::from_secs(600);
/// La taille maximale d'une réponse retenue — les mutations répondent par
/// quelques centaines d'octets de JSON.
const CORPS_MAX: usize = 256 * 1024;
/// La longueur maximale d'une clé.
const CLE_MAX: usize = 128;

#[derive(Clone)]
enum Entree {
    EnCours,
    Rendue {
        statut: StatusCode,
        content_type: Option<HeaderValue>,
        corps: axum::body::Bytes,
    },
}

/// Le registre des clés vues — partagé par toutes les mutations du routeur.
#[derive(Default)]
pub struct Idempotence {
    entrees: Mutex<HashMap<String, (Instant, Entree)>>,
}

impl Idempotence {
    #[must_use]
    pub fn nouveau() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Réserve la clé, ou rend ce qui est déjà connu d'elle.
    fn reserve(&self, cle: &str) -> Option<Entree> {
        let mut entrees = self.entrees.lock().unwrap_or_else(PoisonError::into_inner);
        let maintenant = Instant::now();
        entrees.retain(|_, (depuis, _)| maintenant.duration_since(*depuis) < DUREE_DE_VIE);
        if let Some((_, entree)) = entrees.get(cle) {
            return Some(entree.clone());
        }
        // Plein : la plus ancienne RENDUE part (une en cours n'est jamais
        // évincée — sa répétition doit rester un 409).
        if entrees.len() >= CLES_MAX
            && let Some(ancienne) = entrees
                .iter()
                .filter(|(_, (_, entree))| matches!(entree, Entree::Rendue { .. }))
                .min_by_key(|(_, (depuis, _))| *depuis)
                .map(|(cle, _)| cle.clone())
        {
            entrees.remove(&ancienne);
        }
        entrees.insert(cle.to_string(), (maintenant, Entree::EnCours));
        None
    }

    fn rend(&self, cle: &str, entree: Entree) {
        self.entrees
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cle.to_string(), (Instant::now(), entree));
    }

    fn oublie(&self, cle: &str) {
        self.entrees
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(cle);
    }
}

/// Retire l'entrée « en cours » si la requête n'a pas abouti (abandon,
/// panique) — désarmée quand la réponse est gardée.
struct Reservation {
    registre: Arc<Idempotence>,
    cle: String,
    armee: bool,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.armee {
            self.registre.oublie(&self.cle);
        }
    }
}

fn refus(statut: StatusCode, message: &str) -> Response {
    (statut, axum::Json(serde_json::json!({ "error": message }))).into_response()
}

/// La couche d'idempotence des mutations.
pub async fn idempotence(
    State(registre): State<Arc<Idempotence>>,
    demande: Request,
    suivant: Next,
) -> Response {
    let Some(brute) = demande.headers().get("idempotency-key") else {
        return suivant.run(demande).await;
    };
    let Some(cle) = brute
        .to_str()
        .ok()
        .filter(|cle| !cle.is_empty() && cle.len() <= CLE_MAX)
    else {
        return refus(
            StatusCode::BAD_REQUEST,
            "invalid Idempotency-Key: 1 to 128 visible ASCII characters",
        );
    };
    // La clé vaut pour CETTE route : la même clé ailleurs est une autre action.
    let cle = format!("{} {}\u{0}{cle}", demande.method(), demande.uri().path());
    match registre.reserve(&cle) {
        Some(Entree::EnCours) => {
            return refus(
                StatusCode::CONFLICT,
                "the same request is still running — wait for its answer; nothing was repeated",
            );
        }
        Some(Entree::Rendue {
            statut,
            content_type,
            corps,
        }) => {
            let mut reponse = Response::new(Body::from(corps));
            *reponse.status_mut() = statut;
            if let Some(content_type) = content_type {
                reponse
                    .headers_mut()
                    .insert(axum::http::header::CONTENT_TYPE, content_type);
            }
            reponse
                .headers_mut()
                .insert("idempotent-replayed", HeaderValue::from_static("true"));
            return reponse;
        }
        None => {}
    }
    let mut reservation = Reservation {
        registre: Arc::clone(&registre),
        cle: cle.clone(),
        armee: true,
    };
    let reponse = suivant.run(demande).await;
    let (parties, corps) = reponse.into_parts();
    let corps = match axum::body::to_bytes(corps, CORPS_MAX).await {
        Ok(corps) => corps,
        // Trop gros ou illisible : rien n'est gardé (la réservation tombe), et
        // la cause est dite plutôt qu'un corps tronqué.
        Err(cause) => {
            return refus(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("the response could not be buffered for Idempotency-Key: {cause}"),
            );
        }
    };
    registre.rend(
        &cle,
        Entree::Rendue {
            statut: parties.status,
            content_type: parties
                .headers
                .get(axum::http::header::CONTENT_TYPE)
                .cloned(),
            corps: corps.clone(),
        },
    );
    reservation.armee = false;
    Response::from_parts(parties, Body::from(corps))
}
