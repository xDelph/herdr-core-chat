//! `POST /api/agents/:paneId/attachments?name=photo.jpg` (H26) : une photo
//! ou une capture posée **à côté du terminal** de l'agent (`terminal.put`,
//! 8 Mo au plus), pour que son programme la lise.
//!
//! ⚠️ **Rien n'est tapé.** La réponse rend le chemin ; c'est le message de
//! l'utilisateur qui le porte, comme un collage d'image sur le Mac. Le
//! fichier vit dans un dossier propre au terminal et part avec lui.
//!
//! ⚠️ **Des octets bruts, pas du JSON** : la route a sa propre garde
//! (origine, `image/*` ou `application/octet-stream`, taille). Un
//! formulaire HTML ne peut produire aucun de ces types.

use crate::services::Services;
use axum::response::IntoResponse;
use contracts::http::commands::AttachmentResponse;
use contracts::limits::MAX_ATTACHMENT_BYTES;
use contracts::registry::RouteId;
use domain::errors::ErreurDomaine;
use domain::ids::PaneId;
use std::sync::Arc;

/// La longueur maximale d'un nom gardé.
const NOM_MAX: usize = 100;

/// Le nom du fichier, assaini : le dernier segment, les seuls caractères
/// `A-Z a-z 0-9 . _ -`, jamais caché (`.x`), 100 au plus. Vide : `photo`
/// plus l'extension que le type annonce.
#[must_use]
pub fn nom_assaini(brut: Option<&str>, content_type: Option<&str>) -> String {
    let dernier = brut
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let mut nom: String = dernier
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    while nom.starts_with('.') {
        nom.remove(0);
    }
    if nom.trim_matches('_').is_empty() {
        let extension = match content_type
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            t if t.starts_with("image/png") => ".png",
            t if t.starts_with("image/jpeg") => ".jpg",
            t if t.starts_with("image/heic") => ".heic",
            t if t.starts_with("image/gif") => ".gif",
            t if t.starts_with("image/webp") => ".webp",
            _ => "",
        };
        nom = format!("photo{extension}");
    }
    if nom.len() > NOM_MAX {
        // Garder l'extension : `.png` doit rester `.png`.
        let extension = nom
            .rfind('.')
            .map(|i| nom[i..].to_string())
            .filter(|e| e.len() <= 10)
            .unwrap_or_default();
        nom.truncate(NOM_MAX - extension.len());
        nom.push_str(&extension);
    }
    nom
}

#[derive(serde::Deserialize)]
pub struct Requete {
    pub name: Option<String>,
}

async fn depose_sur_place(
    services: Arc<Services>,
    pane_id: String,
    nom: String,
    octets: Vec<u8>,
) -> axum::response::Response {
    if octets.is_empty() {
        return crate::handlers::repondre_erreur(
            RouteId::PieceJointe,
            &ErreurDomaine::entree_invalide("empty attachment: nothing was sent"),
        );
    }
    let taille = octets.len();
    let mut futur = services
        .port
        .depose(PaneId::from(pane_id.clone()), nom.clone(), octets);
    match application::commands::attend_port(&mut futur) {
        Ok(depose) => {
            tracing::info!(pane = %pane_id, octets = taille, "pièce jointe posée à côté du terminal");
            (
                axum::http::StatusCode::OK,
                axum::Json(AttachmentResponse {
                    path: depose.chemin,
                    name: nom,
                    bytes: depose.octets,
                }),
            )
                .into_response()
        }
        Err(cause) => crate::handlers::repondre_erreur(
            RouteId::PieceJointe,
            &ErreurDomaine::amont_refus(format!(
                "the attachment was not delivered: {}",
                cause.description()
            ))
            .avec_port(&cause),
        ),
    }
}

/// Le handler `piece_jointe` : le corps borné à 8 Mo (la taille RÉELLE),
/// puis le dépôt hors des workers du runtime.
pub async fn piece_jointe(
    axum::extract::State(services): axum::extract::State<Arc<Services>>,
    axum::extract::Path(pane_id): axum::extract::Path<String>,
    axum::extract::Query(requete): axum::extract::Query<Requete>,
    demande: axum::extract::Request,
) -> axum::response::Response {
    let content_type = demande
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|valeur| valeur.to_str().ok())
        .map(String::from);
    let nom = nom_assaini(requete.name.as_deref(), content_type.as_deref());
    let octets =
        match crate::corps::lire_corps_borne(demande.into_body(), MAX_ATTACHMENT_BYTES).await {
            Ok(octets) => octets,
            Err(refus) => return refus,
        };
    crate::bloquant::hors_runtime(depose_sur_place(services, pane_id, nom, octets)).await
}
