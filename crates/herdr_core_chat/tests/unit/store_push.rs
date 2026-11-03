//! E08/E10 — le store push tel que le démon le compose
//! (`assemblage::store_push`, appelé par `main`) : clés VAPID de
//! l'environnement appliquées, paire partielle refusée, et **aucun** import
//! du store d'Herdr Chat (`.herdr-remote/push.json`, AGENTS §5).

use herdr_core_chat::assemblage::store_push;
use herdr_core_chat::config::{Config, LanceStatutServe, config_depuis_avec};
use notifications::store::{AbonnementPush, ClefsAbonnement, StorePush, ecrit_store, lit_store};
use notifications::vapid::genere_clefs_vapid;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct SansTailscale;

impl LanceStatutServe for SansTailscale {
    fn lance(&self, _argv: &[&str], _delai: Duration) -> Result<String, String> {
        Err(String::from("pas de tailscale dans les tests"))
    }
}

static COMPTEUR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Un dossier jetable : jamais le HOME, jamais le store réel.
fn dossier(prefixe: &str) -> PathBuf {
    let chemin = std::env::temp_dir().join(format!(
        "{prefixe}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duree| duree.as_nanos())
            .unwrap_or(0),
        COMPTEUR.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&chemin).unwrap_or_else(|cause| panic!("dossier : {cause}"));
    chemin
}

fn config(store: &Path, env_vapid: Option<(String, String)>) -> Config {
    let mut env = HashMap::new();
    env.insert(
        String::from("HERDR_CORE_CHAT_PUSH_STORE"),
        store.to_string_lossy().into_owned(),
    );
    if let Some((publique, privee)) = env_vapid {
        env.insert(String::from("HERDR_CORE_CHAT_VAPID_PUBLIC_KEY"), publique);
        env.insert(String::from("HERDR_CORE_CHAT_VAPID_PRIVATE_KEY"), privee);
    }
    config_depuis_avec(&env, Path::new("/foyer"), &SansTailscale)
        .unwrap_or_else(|cause| panic!("config : {cause:?}"))
}

#[test]
fn les_cles_vapid_de_l_environnement_sont_celles_servies() {
    let racine = dossier("herdr-store-env");
    let clefs = genere_clefs_vapid();
    let config = config(
        &racine.join("push.json"),
        Some((clefs.public_key.clone(), clefs.private_key.clone())),
    );
    let gardien = store_push(&config).unwrap_or_else(|cause| panic!("{cause}"));
    assert_eq!(
        gardien
            .cle_publique()
            .unwrap_or_else(|erreur| panic!("{erreur:?}")),
        clefs.public_key,
        "la clé de l'environnement prime sur une clé générée"
    );
    let _ = std::fs::remove_dir_all(&racine);
}

#[test]
fn une_paire_vapid_melangee_est_refusee_au_demarrage() {
    let racine = dossier("herdr-store-melange");
    let une = genere_clefs_vapid();
    let autre = genere_clefs_vapid();
    let config = config(
        &racine.join("push.json"),
        Some((une.public_key, autre.private_key)),
    );
    match store_push(&config) {
        Err(cause) => assert!(cause.contains("VAPID"), "{cause}"),
        Ok(_) => panic!("une paire mélangée ne doit jamais être complétée en silence"),
    }
    let _ = std::fs::remove_dir_all(&racine);
}

#[test]
fn le_store_d_herdr_chat_n_est_jamais_importe() {
    // Porte capable d'échouer : un store d'Herdr Chat existe, avec clés et
    // abonnement, dans le HOME simulé. Le nôtre doit naître VIDE.
    let maison = dossier("herdr-store-coexistence");
    let ancien = maison.join(".herdr-remote").join("push.json");
    std::fs::create_dir_all(ancien.parent().unwrap_or(&maison))
        .unwrap_or_else(|cause| panic!("{cause}"));
    let mut store = StorePush::vide();
    store.vapid_keys = Some(genere_clefs_vapid());
    store.subscriptions.push(AbonnementPush {
        endpoint: String::from("https://push.example/telephone-herdr-chat"),
        keys: ClefsAbonnement {
            p256dh: String::from("p256dh"),
            auth: String::from("auth"),
        },
    });
    ecrit_store(&ancien, &store).unwrap_or_else(|erreur| panic!("{erreur:?}"));
    let config = config_depuis_avec(&HashMap::new(), &maison, &SansTailscale)
        .unwrap_or_else(|cause| panic!("config : {cause:?}"));
    assert_eq!(
        config.push_store_path,
        maison.join(".herdr-core-chat").join("push.json")
    );
    let gardien = store_push(&config).unwrap_or_else(|cause| panic!("{cause}"));
    let abonnements = gardien
        .abonnements()
        .unwrap_or_else(|erreur| panic!("{erreur:?}"));
    assert!(abonnements.is_empty(), "abonnements d'Herdr Chat importés");
    let cle = gardien
        .cle_publique()
        .unwrap_or_else(|erreur| panic!("{erreur:?}"));
    assert_ne!(
        Some(cle),
        store.vapid_keys.map(|clefs| clefs.public_key),
        "clé VAPID d'Herdr Chat reprise"
    );
    let relu = lit_store(&ancien).unwrap_or_else(|erreur| panic!("{erreur:?}"));
    assert_eq!(
        relu.subscriptions.len(),
        1,
        "le store d'Herdr Chat est intact"
    );
    let _ = std::fs::remove_dir_all(&maison);
}
