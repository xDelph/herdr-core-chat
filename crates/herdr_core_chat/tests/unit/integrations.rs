//! Les agents installés sous herdr-core : cherchés dans le `PATH` configuré,
//! jamais omis — un absent est `not_installed`.

use herdr_core_chat::integrations::integrations_par_path;
use std::os::unix::fs::PermissionsExt;

fn dossier(nom: &str) -> std::path::PathBuf {
    let chemin =
        std::env::temp_dir().join(format!("hcc-integrations-{nom}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&chemin);
    std::fs::create_dir_all(&chemin).unwrap_or_else(|cause| panic!("{cause}"));
    chemin
}

fn etat(valeur: &serde_json::Value, commande: &str) -> String {
    valeur["integrations"]
        .as_array()
        .and_then(|liste| liste.iter().find(|i| i["command"] == commande))
        .and_then(|i| i["state"].as_str())
        .unwrap_or_else(|| panic!("{commande} absent de la liste"))
        .to_string()
}

#[test]
fn un_executable_du_path_est_installe_les_autres_non() {
    let bin = dossier("bin");
    let pi = bin.join("pi");
    std::fs::write(&pi, "#!/bin/sh\n").unwrap_or_else(|cause| panic!("{cause}"));
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|cause| panic!("{cause}"));
    // Un fichier NON exécutable ne compte pas.
    std::fs::write(bin.join("codex"), "pas un programme").unwrap_or_else(|cause| panic!("{cause}"));
    let valeur = integrations_par_path(&format!("/inexistant:{}", bin.display()));
    assert_eq!(etat(&valeur, "pi"), "current");
    assert_eq!(etat(&valeur, "codex"), "not_installed");
    assert_eq!(etat(&valeur, "claude"), "not_installed");
    // Cursor : commande `cursor-agent`, kind `cursor`.
    let cursor = valeur["integrations"]
        .as_array()
        .and_then(|liste| {
            liste
                .iter()
                .find(|i| i["command"] == "cursor-agent")
                .cloned()
        })
        .unwrap_or_else(|| panic!("cursor-agent absent"));
    assert_eq!(cursor["target"], "cursor");
    // La forme est celle que le catalogue lit.
    let installes = catalog::port::installees_depuis(&valeur);
    let kinds: Vec<String> = installes.iter().map(|i| i.kind.to_string()).collect();
    assert_eq!(kinds, ["pi"]);
}

#[test]
fn un_path_vide_ne_voit_aucun_agent() {
    let valeur = integrations_par_path("");
    assert!(catalog::port::installees_depuis(&valeur).is_empty());
    assert_eq!(valeur["integrations"].as_array().map(Vec::len), Some(6));
}
