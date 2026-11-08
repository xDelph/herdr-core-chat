//! H24 — POST `/api/agents/:paneId/model` : modèle et réflexion en session,
//! à travers le routeur composé.
//!
//! ⚠️ Le type d'agent vient de Herdr (`list_agents`), jamais du client : un
//! pane sans agent connu rend 404 et RIEN n'est tapé. Les recettes elles-mêmes
//! sont prouvées dans `crates/interactions/tests/unit/reglage.rs`.

use crate::unit::support::{envoie_post, services};
use axum::http::StatusCode;
use domain::ids::PaneId;
use domain::observation::ScreenObservation;
use http_api::router::routeur;
use serde_json::json;
use test_support::fake_herdr::{HerdrFaux, Op};
use test_support::fixtures::snapshot_de_base;

/// Le faux Herdr et l'instantané de base (p1 pi, p2 codex, p3 pi).
fn faux_de_base() -> HerdrFaux {
    let faux = HerdrFaux::nouveau();
    faux.repond_snapshot(snapshot_de_base());
    faux
}

fn ecran(pane: &str, texte: &str) -> ScreenObservation {
    ScreenObservation {
        pane_id: PaneId::from(String::from(pane)),
        text: Some(String::from(texte)),
        truncated: None,
        source: None,
    }
}

fn textes_tapes(faux: &HerdrFaux) -> Vec<String> {
    faux.actions()
        .into_iter()
        .filter(|action| action.op == Op::EnvoiTexte)
        .map(|action| action.cible)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn codex_200_appliquee_sur_sa_confirmation() {
    let faux = faux_de_base();
    // p2 est un codex dans l'instantané de base.
    faux.remplace_ecran(ecran("p2", "› \n• Model changed to gpt-5 high"));
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/p2/model",
        json!({ "model": "gpt-5", "thinking": "high" })
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(statut, StatusCode::OK, "{corps}");
    assert_eq!(corps["status"], "model-changed");
    assert_eq!(corps["state"], "appliquee");
    assert_eq!(
        textes_tapes(&faux),
        ["p2"],
        "une seule commande, sur LA cible"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sans_confirmation_c_est_transmise_jamais_appliquee() {
    let faux = faux_de_base();
    faux.remplace_ecran(ecran("p1", "π rien de neuf"));
    let (services, _) = services(faux, Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/p1/model",
        json!({ "model": "anthropic/claude-opus-5" })
            .to_string()
            .into_bytes(),
    )
    .await;
    assert_eq!(statut, StatusCode::OK, "{corps}");
    assert_eq!(corps["state"], "transmise");
}

#[tokio::test(flavor = "multi_thread")]
async fn un_pane_sans_agent_connu_est_404_et_rien_n_est_tape() {
    let faux = faux_de_base();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/inconnu/model",
        json!({ "model": "opus" }).to_string().into_bytes(),
    )
    .await;
    assert_eq!(statut, StatusCode::NOT_FOUND, "{corps}");
    assert!(textes_tapes(&faux).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn un_corps_invalide_est_400_et_rien_n_est_tape() {
    let faux = faux_de_base();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    for corps in [
        json!({ "model": 42 }).to_string(),
        json!({ "model": "opus", "inconnu": true }).to_string(),
        String::from("pas du json"),
    ] {
        let (statut, reponse) = envoie_post(
            routeur(services.clone(), vec![]),
            "/api/agents/p2/model",
            corps.clone().into_bytes(),
        )
        .await;
        assert_eq!(statut, StatusCode::BAD_REQUEST, "{corps} → {reponse}");
    }
    assert!(textes_tapes(&faux).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn un_refus_de_recette_est_une_valeur_du_200() {
    let faux = faux_de_base();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    // Une valeur de plusieurs mots ne part jamais : collée puis validée par
    // Entrée, elle ferait passer une seconde commande.
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/p2/model",
        json!({ "model": "gpt-5\n/clear" }).to_string().into_bytes(),
    )
    .await;
    assert_eq!(statut, StatusCode::OK, "{corps}");
    assert_eq!(corps["state"], "refusee");
    assert!(textes_tapes(&faux).is_empty());
}
