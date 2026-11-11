//! `Idempotency-Key` sur les mutations : une répétition du transport n'est
//! jamais ré-exécutée — elle reçoit la même réponse, rejouée.

use crate::unit::support::{agent, borne, services};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::status::AgentStatus;
use http_api::router::routeur;
use test_support::fake_herdr::{HerdrFaux, Op};
use tower::ServiceExt;

async fn poste(
    routeur: axum::Router<()>,
    cle: Option<&str>,
    texte: &str,
) -> (StatusCode, serde_json::Value, bool) {
    let mut demande = Request::builder()
        .method("POST")
        .uri("/api/agents/w1:p1/prompt")
        .header("origin", "http://localhost")
        .header("content-type", "application/json");
    if let Some(cle) = cle {
        demande = demande.header("idempotency-key", cle);
    }
    let reponse = routeur
        .oneshot(
            demande
                .body(Body::from(serde_json::json!({ "text": texte }).to_string()))
                .unwrap_or_else(|cause| panic!("requête : {cause:?}")),
        )
        .await
        .unwrap_or_else(|cause| panic!("réponse : {cause:?}"));
    let statut = reponse.status();
    let rejouee = reponse.headers().contains_key("idempotent-replayed");
    let octets = axum::body::to_bytes(reponse.into_body(), 64 * 1024)
        .await
        .unwrap_or_else(|cause| panic!("corps : {cause:?}"));
    let valeur = serde_json::from_slice(&octets).unwrap_or(serde_json::Value::Null);
    (statut, valeur, rejouee)
}

fn faux_au_travail() -> HerdrFaux {
    let faux = HerdrFaux::nouveau();
    let mut observation = agent("w1:p1", "claude", None);
    observation.status = AgentStatus::Working;
    faux.repond_snapshot(borne(vec![observation]).snapshot);
    faux
}

fn prompts(faux: &HerdrFaux) -> usize {
    faux.actions()
        .iter()
        .filter(|action| action.op == Op::Prompt)
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn la_meme_cle_rejoue_la_reponse_sans_refaire_l_envoi() {
    let faux = faux_au_travail();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let routeur = routeur(services, vec![]);
    let (statut, premiere, rejouee) = poste(routeur.clone(), Some("cle-1"), "bonjour").await;
    assert_eq!(statut, StatusCode::OK, "{premiere}");
    assert_eq!(premiere["status"], "sent");
    assert!(!rejouee);
    // B1.15 : l'agent travaillait — le message est dans sa file.
    assert_eq!(premiere["queued"], true, "{premiere}");
    let (statut, seconde, rejouee) = poste(routeur.clone(), Some("cle-1"), "bonjour").await;
    assert_eq!(statut, StatusCode::OK);
    assert!(rejouee, "la réponse est rejouée");
    assert_eq!(seconde, premiere, "la même réponse, pas un « suppressed »");
    assert_eq!(prompts(&faux), 1, "un seul envoi à Herdr");
}

#[tokio::test(flavor = "multi_thread")]
async fn une_autre_cle_ou_pas_de_cle_est_une_requete_neuve() {
    let faux = faux_au_travail();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let routeur = routeur(services, vec![]);
    let _ = poste(routeur.clone(), Some("cle-a"), "un").await;
    let _ = poste(routeur.clone(), Some("cle-b"), "deux").await;
    let _ = poste(routeur.clone(), None, "trois").await;
    assert_eq!(prompts(&faux), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn une_cle_invalide_est_refusee_sans_rien_envoyer() {
    let faux = faux_au_travail();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let longue = "x".repeat(200);
    let (statut, corps, _) = poste(routeur(services, vec![]), Some(&longue), "bonjour").await;
    assert_eq!(statut, StatusCode::BAD_REQUEST, "{corps}");
    assert_eq!(prompts(&faux), 0);
}
