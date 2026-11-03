//! Les origines **redécouvertes** : un démon né sans l'origine Tailscale ne
//! reste pas aveugle. Retour du 2026-11-03 (« pour la énième fois, la PWA se
//! voit rejeter à cause de l'origin ») — mesuré : `tailscale serve status` a
//! dépassé son délai au démarrage du 2026-11-02 20:18, et chaque envoi de la
//! PWA a pris un 403 jusqu'au redémarrage suivant.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_api::origin::{INTERVALLE_REDECOUVERTE, OriginesAutorisees, Redecouverte};
use http_api::router::routeur;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use test_support::fake_herdr::HerdrFaux;
use tower::ServiceExt;

use crate::unit::support::services;

const LOCALE: &str = "http://127.0.0.1:8791";
const TAILNET: &str = "https://macbook.tailnet.ts.net:8443";

/// Une découverte qui compte ses appels et répond ce qu'on lui donne.
fn decouverte(reponse: Vec<String>) -> (Redecouverte, Arc<AtomicUsize>) {
    let appels = Arc::new(AtomicUsize::new(0));
    let compteur = Arc::clone(&appels);
    let redecouverte: Redecouverte = Arc::new(move || {
        compteur.fetch_add(1, Ordering::SeqCst);
        reponse.clone()
    });
    (redecouverte, appels)
}

async fn poste_journal(origines: OriginesAutorisees, origine: &str) -> StatusCode {
    let (services, _) = services(HerdrFaux::nouveau(), Default::default(), Default::default());
    routeur(services, origines)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/log")
                .header("origin", origine)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"level":"error","message":"x"}"#))
                .unwrap_or_else(|cause| panic!("requête construite : {cause:?}")),
        )
        .await
        .unwrap_or_else(|cause| panic!("réponse reçue : {cause:?}"))
        .status()
}

#[tokio::test(flavor = "multi_thread")]
async fn l_origine_tailscale_manquee_au_demarrage_est_redecouverte() {
    // Le démarrage a raté Tailscale : seule l'origine locale est connue.
    let (redecouverte, appels) = decouverte(vec![TAILNET.to_string()]);
    let origines = OriginesAutorisees::avec_redecouverte(
        vec![LOCALE.to_string()],
        redecouverte,
        INTERVALLE_REDECOUVERTE,
    );
    assert_eq!(
        poste_journal(origines.clone(), TAILNET).await,
        StatusCode::OK
    );
    assert_eq!(appels.load(Ordering::SeqCst), 1);
    // Trouvée, elle reste : la requête suivante ne relance rien.
    assert_eq!(poste_journal(origines, TAILNET).await, StatusCode::OK);
    assert_eq!(appels.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn une_origine_etrangere_reste_refusee_apres_redecouverte() {
    let (redecouverte, appels) = decouverte(vec![TAILNET.to_string()]);
    let origines = OriginesAutorisees::avec_redecouverte(
        vec![LOCALE.to_string()],
        redecouverte,
        INTERVALLE_REDECOUVERTE,
    );
    // Comparaison exacte, jamais un motif : un suffixe ne passe pas.
    for etrangere in [
        "https://evil.example.com",
        "https://macbook.tailnet.ts.net.evil.com:8443",
    ] {
        assert_eq!(
            poste_journal(origines.clone(), etrangere).await,
            StatusCode::FORBIDDEN
        );
    }
    // Et la rafale n'a lancé qu'UNE découverte (l'intervalle la borne).
    assert_eq!(appels.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn sans_redecouverte_la_liste_reste_figee() {
    let origines = OriginesAutorisees::fixes(vec![LOCALE.to_string()]);
    assert_eq!(
        poste_journal(origines, TAILNET).await,
        StatusCode::FORBIDDEN
    );
}

#[test]
fn tailscale_muet_n_efface_pas_une_origine_deja_trouvee() {
    let reponses = Arc::new(std::sync::Mutex::new(vec![
        Vec::new(),
        vec![TAILNET.to_string()],
    ]));
    let file = Arc::clone(&reponses);
    let origines = OriginesAutorisees::avec_redecouverte(
        vec![LOCALE.to_string()],
        Arc::new(move || {
            file.lock()
                .map(|mut f| f.pop().unwrap_or_default())
                .unwrap_or_default()
        }),
        Duration::ZERO,
    );
    assert!(origines.rafraichit());
    assert_eq!(origines.liste(), [LOCALE, TAILNET]);
    // La découverte suivante revient vide : rien n'est retiré.
    assert!(!origines.rafraichit());
    assert_eq!(origines.liste(), [LOCALE, TAILNET]);
}

#[test]
fn l_intervalle_borne_les_redecouvertes() {
    let (redecouverte, appels) = decouverte(Vec::new());
    let origines = OriginesAutorisees::avec_redecouverte(
        vec![LOCALE.to_string()],
        redecouverte,
        Duration::from_secs(3600),
    );
    for _ in 0..5 {
        assert!(!origines.rafraichit());
    }
    assert_eq!(appels.load(Ordering::SeqCst), 1);
}
