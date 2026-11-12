//! Le notifieur : un payload métier → une livraison enfilée vers TOUS les
//! abonnements du store, sans jamais attendre le réseau.

use notifications::delivery::{ErreurLivraison, LivraisonPush};
use notifications::notifieur::{ErreurNotification, Notifieur, texte_de_notification};
use notifications::payload::{StatutDeFin, payload_de_notification};
use notifications::store::{AbonnementPush, ClefsAbonnement, GardienDeStore};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

static COMPTEUR: AtomicU64 = AtomicU64::new(0);

fn store() -> Arc<GardienDeStore> {
    let dossier = std::env::temp_dir().join(format!(
        "herdr-notifieur-{}-{}",
        std::process::id(),
        COMPTEUR.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dossier).unwrap_or_else(|cause| panic!("dossier : {cause}"));
    Arc::new(GardienDeStore::nouveau(dossier.join("push.json"), None))
}

fn abonnement(endpoint: &str) -> AbonnementPush {
    AbonnementPush {
        endpoint: String::from(endpoint),
        keys: ClefsAbonnement {
            p256dh: String::from("p256dh"),
            auth: String::from("auth"),
        },
    }
}

/// Le faux service : note (endpoint, payload).
#[derive(Default)]
struct FauxService {
    envois: Mutex<Vec<(String, String)>>,
}

impl FauxService {
    fn envois(&self) -> Vec<(String, String)> {
        self.envois
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn attend(&self, attendus: usize) {
        let debut = std::time::Instant::now();
        while self.envois().len() < attendus {
            assert!(
                debut.elapsed() < Duration::from_secs(2),
                "{attendus} envois attendus, {} vus",
                self.envois().len()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

impl LivraisonPush for FauxService {
    fn envoie<'a>(
        &'a self,
        abonnement: &'a AbonnementPush,
        payload: &'a str,
        _ttl: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErreurLivraison>> + Send + 'a>> {
        Box::pin(async move {
            self.envois
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((abonnement.endpoint.clone(), String::from(payload)));
            Ok(())
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sans_abonne_rien_n_est_enfile() {
    let service = Arc::new(FauxService::default());
    let notifieur = Notifieur::nouveau(service.clone(), store());
    let payload = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::Fini);
    let vises = notifieur
        .notifie(&payload)
        .unwrap_or_else(|erreur| panic!("{erreur}"));
    assert_eq!(vises, 0);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(service.envois().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chaque_abonne_recoit_le_payload_du_fil() {
    let service = Arc::new(FauxService::default());
    let gardien = store();
    for endpoint in ["https://push.example/1", "https://push.example/2"] {
        gardien
            .abonne(&abonnement(endpoint))
            .unwrap_or_else(|erreur| panic!("abonnement : {erreur:?}"));
    }
    let notifieur = Notifieur::nouveau(service.clone(), gardien);
    let payload = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::EnAttente);
    let vises = notifieur
        .notifie(&payload)
        .unwrap_or_else(|erreur| panic!("{erreur}"));
    assert_eq!(vises, 2);
    service.attend(2).await;
    for (_, texte) in service.envois() {
        assert_eq!(texte, texte_de_notification(&payload));
        let json: serde_json::Value =
            serde_json::from_str(&texte).unwrap_or_else(|cause| panic!("{cause}"));
        assert_eq!(json["title"], "Agent needs you");
        assert_eq!(json["tag"], "waiting-w1:p1");
        assert_eq!(json["url"], "/#/agent/w1%3Ap1");
        assert_eq!(json["body"], "pixel is waiting for your answer.");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn un_store_illisible_est_une_cause_nommee() {
    let dossier = std::env::temp_dir().join(format!(
        "herdr-notifieur-casse-{}-{}",
        std::process::id(),
        COMPTEUR.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dossier).unwrap_or_else(|cause| panic!("dossier : {cause}"));
    let chemin = dossier.join("push.json");
    std::fs::write(&chemin, b"{pas du json").unwrap_or_else(|cause| panic!("{cause}"));
    let notifieur = Notifieur::nouveau(
        Arc::new(FauxService::default()),
        Arc::new(GardienDeStore::nouveau(chemin, None)),
    );
    let payload = payload_de_notification("", Some("w1:p1"), StatutDeFin::Fini);
    assert!(matches!(
        notifieur.notifie(&payload),
        Err(ErreurNotification::Store(_))
    ));
}

/// H23 — une PWA au premier plan retient les notifications pour TOUS les
/// appareils, pas seulement pour celui où elle tourne. Retour du 2026-10-29 :
/// « je suis dans l'app, et je reçois quand même la notif » ; 2026-11-12 : PWA
/// ouverte sur la tablette, téléphone notifié quand même.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn une_pwa_au_premier_plan_retient_toutes_les_notifications() {
    use notifications::presence::PresenceAppareils;
    let service = Arc::new(FauxService::default());
    let gardien = store();
    for endpoint in [
        "https://push.example/telephone",
        "https://push.example/ipad",
    ] {
        gardien
            .abonne(&abonnement(endpoint))
            .unwrap_or_else(|erreur| panic!("abonnement : {erreur:?}"));
    }
    let presence = Arc::new(PresenceAppareils::nouvelle());
    presence.marque(
        "https://push.example/telephone",
        true,
        std::time::Instant::now(),
    );
    let notifieur =
        Notifieur::nouveau(service.clone(), gardien).avec_presence(Arc::clone(&presence));
    let payload = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::Fini);
    assert_eq!(notifieur.notifie(&payload).ok(), Some(0));
    service.attend(0).await;
    assert!(service.envois().is_empty(), "rien ne part, ni vers l'iPad");

    // La PWA quitte le premier plan : tout le monde est de nouveau prévenu.
    presence.marque(
        "https://push.example/telephone",
        false,
        std::time::Instant::now(),
    );
    assert_eq!(notifieur.notifie(&payload).ok(), Some(2));
}

/// Retour du 2026-11-12 : la PWA ouverte sur la tablette (sans notifications
/// activées, donc sans abonnement), le téléphone abonné ne doit RIEN recevoir.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn une_pwa_ouverte_sur_un_appareil_sans_abonnement_retient_tout() {
    use notifications::presence::PresenceAppareils;
    let service = Arc::new(FauxService::default());
    let gardien = store();
    gardien
        .abonne(&abonnement("https://push.example/telephone"))
        .unwrap_or_else(|erreur| panic!("abonnement : {erreur:?}"));
    let presence = Arc::new(PresenceAppareils::nouvelle());
    presence.marque("tablette-sans-abonnement", true, std::time::Instant::now());
    let notifieur =
        Notifieur::nouveau(service.clone(), gardien).avec_presence(Arc::clone(&presence));
    let payload = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::Fini);
    assert_eq!(notifieur.notifie(&payload).ok(), Some(0));
    service.attend(0).await;
    assert!(service.envois().is_empty(), "rien ne part");

    // La tablette quitte l'app : le téléphone est de nouveau prévenu.
    presence.marque("tablette-sans-abonnement", false, std::time::Instant::now());
    assert_eq!(notifieur.notifie(&payload).ok(), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn un_genre_coupe_dans_les_reglages_ne_part_pas_vers_cet_appareil() {
    let service = Arc::new(FauxService::default());
    let gardien = store();
    for endpoint in ["https://push.example/1", "https://push.example/2"] {
        gardien
            .abonne(&abonnement(endpoint))
            .unwrap_or_else(|erreur| panic!("abonnement : {erreur:?}"));
    }
    // L'appareil 1 ne veut plus « fini » ; il veut toujours « à toi ».
    gardien
        .coupe("https://push.example/1", &[String::from("done")])
        .unwrap_or_else(|erreur| panic!("coupe : {erreur:?}"));
    let notifieur = Notifieur::nouveau(service.clone(), Arc::clone(&gardien));

    let fini = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::Fini);
    assert_eq!(fini.genre, "done");
    let vises = notifieur
        .notifie(&fini)
        .unwrap_or_else(|erreur| panic!("{erreur}"));
    assert_eq!(vises, 1);
    service.attend(1).await;
    assert_eq!(service.envois()[0].0, "https://push.example/2");

    let attente = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::EnAttente);
    let vises = notifieur
        .notifie(&attente)
        .unwrap_or_else(|erreur| panic!("{erreur}"));
    assert_eq!(vises, 2);

    // Désabonné : ses réglages partent avec lui.
    gardien
        .desabonne("https://push.example/1")
        .unwrap_or_else(|erreur| panic!("désabonnement : {erreur:?}"));
    assert!(
        gardien
            .coupes()
            .unwrap_or_else(|erreur| panic!("{erreur:?}"))
            .is_empty()
    );
}

#[test]
fn le_genre_ne_sort_pas_dans_le_json_envoye() {
    let payload = payload_de_notification("pixel", Some("w1:p1"), StatutDeFin::Erreur);
    assert_eq!(payload.genre, "error");
    assert!(!texte_de_notification(&payload).contains("genre"));
}
