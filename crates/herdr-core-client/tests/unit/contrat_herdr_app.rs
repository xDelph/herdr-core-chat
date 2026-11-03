//! Contrat de lecture des formats de Herdr.app (AGENTS §4) — sur les
//! fixtures VERSIONNÉES relevées le 2026-11-03 (`tests/fixtures/herdr-app-0.1.12`).
//! Un changement de format chez Herdr.app fait échouer ce fichier.

use super::faux_core::hello;
use herdr_core_client::client::{HelloResult, Terminal};
use herdr_core_client::disposition::disposition_depuis;
use herdr_core_client::port::snapshot_depuis;
use herdr_core_client::workspace_state::WorkspaceState;
use serde_json::{Value, json};
use std::path::PathBuf;

fn fixture(nom: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/herdr-app-0.1.12")
        .join(nom)
}

fn json_de(nom: &str) -> Value {
    let texte = std::fs::read_to_string(fixture(nom))
        .unwrap_or_else(|cause| panic!("fixture {nom} : {cause}"));
    serde_json::from_str(&texte).unwrap_or_else(|cause| panic!("fixture {nom} : {cause}"))
}

fn hello_lu() -> HelloResult {
    let brut = hello(1);
    HelloResult {
        generation: 1,
        version: brut["version"].as_str().unwrap_or_default().to_string(),
        features: Vec::new(),
        methods: Vec::new(),
    }
}

fn terminal(adresse: &str, cwd: &str) -> Terminal {
    Terminal {
        address: adresse.to_string(),
        name: None,
        argv: vec![String::from("/bin/zsh")],
        cwd: cwd.to_string(),
        state: String::from("live"),
        status: Some(json!({"app":"claude-code","state":"idle"})),
        turn: 1,
        title: String::new(),
        labels: std::collections::BTreeMap::new(),
        parent: None,
        foreground_pid: Some(1),
        foreground_cwd: Some(cwd.to_string()),
        foreground_name: Some(String::from("claude")),
    }
}

#[test]
fn le_doc_herdr_app_reel_se_lit() {
    let doc = json_de("doc-herdr-app.json");
    let disposition = disposition_depuis(&doc).unwrap_or_else(|cause| panic!("{cause}"));
    assert_eq!(disposition.spaces.len(), 2);
    let onglets: Vec<usize> = disposition.spaces.iter().map(|s| s.onglets.len()).collect();
    assert_eq!(onglets, [3, 2]);
    let terminaux: Vec<String> = disposition
        .spaces
        .iter()
        .flat_map(|s| s.onglets.iter().flat_map(|o| o.terminaux.clone()))
        .collect();
    assert_eq!(terminaux, ["t1", "t2", "t3", "t4", "t5"]);
    // Les noms réels sont VIDES : Herdr.app les calcule (d'où l'état du mod).
    assert!(disposition.spaces.iter().all(|s| s.name.is_empty()));
}

#[test]
fn l_etat_du_mod_reel_se_lit_et_nomme_les_espaces_du_doc() {
    let etat = match WorkspaceState::charger(&fixture("workspace-state.json")) {
        Ok(Some(etat)) => etat,
        autre => panic!("état du mod illisible : {autre:?}"),
    };
    let doc = json_de("doc-herdr-app.json");
    let disposition = disposition_depuis(&doc).unwrap_or_else(|cause| panic!("{cause}"));
    // Les identifiants du mod sont ceux du doc : c'est ce qui permet de nommer.
    for espace in &disposition.spaces {
        assert!(
            etat.spaces.contains_key(&espace.id),
            "espace {} sans nom",
            espace.id
        );
        for onglet in &espace.onglets {
            assert!(
                etat.tabs.contains_key(&onglet.id),
                "onglet {} sans nom",
                onglet.id
            );
        }
    }
    let terms: Vec<Terminal> = (1..=5)
        .map(|n| terminal(&format!("t{n}"), "/Users/moi/projets/projet-1"))
        .collect();
    let snapshot = snapshot_depuis(&terms, &disposition, Some(&etat), hello_lu());
    let noms: Vec<Option<String>> = snapshot
        .workspaces
        .iter()
        .map(|w| w.label.clone())
        .collect();
    assert_eq!(
        noms,
        [
            Some(String::from("projet-1")),
            Some(String::from("projet-2"))
        ]
    );
    let onglets: Vec<Option<String>> = snapshot
        .tabs
        .unwrap_or_default()
        .iter()
        .map(|t| t.label.clone())
        .collect();
    assert_eq!(onglets.len(), 5);
    assert_eq!(onglets[0].as_deref(), Some("1"));
    assert_eq!(snapshot.agents.len(), 5);
}

#[test]
fn un_etat_du_mod_de_version_inconnue_est_une_erreur_dite() {
    let dossier = super::faux_core::dossier_temporaire("etat-v2");
    let chemin = dossier.join("workspace-state.json");
    let mut etat = json_de("workspace-state.json");
    etat["version"] = json!(2);
    if let Err(cause) = std::fs::write(&chemin, etat.to_string()) {
        panic!("{cause}");
    }
    match WorkspaceState::charger(&chemin) {
        Err(cause) => assert!(cause.contains("version"), "{cause}"),
        Ok(lu) => panic!("version inconnue acceptée : {lu:?}"),
    }
    // Absent : état normal (Herdr.app fermé), pas une erreur.
    match WorkspaceState::charger(&dossier.join("absent.json")) {
        Ok(None) => {}
        autre => panic!("absent : {autre:?}"),
    }
}
