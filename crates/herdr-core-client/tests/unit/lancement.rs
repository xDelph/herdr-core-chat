//! Le lancement (P7) sur un faux core : un onglet = le shell de la machine
//! avec les labels de Herdr.app ; l'agent est tapé dans CE shell ; on ne
//! tape ni ne ferme jamais un terminal qu'on n'a pas créé.

use super::faux_core::{Reponse, attends, demandes_de, demarre, dossier_temporaire, hello};
use domain::errors::PortError;
use domain::ids::{AgentKind, PaneId, TabId, WorkspaceId};
use domain::ports::{ControlPort, CreateTab, StartAgent, StartEvidence};
use herdr_core_client::ClientCore;
use herdr_core_client::port::{PortCore, ligne_de_lancement, nom_de_terminal};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// L'état du faux core pendant un lancement.
#[derive(Clone)]
struct Etat {
    ecran: String,
    /// Le PID au premier plan (le shell, puis l'agent après l'envoi).
    pid_avant: u64,
    pid_apres: Option<u64>,
    fini: bool,
    nom_pris: bool,
}

fn faux_core(etat: Etat) -> (PortCore, super::faux_core::Demandes) {
    let etat = Arc::new(Mutex::new((etat, false)));
    let (socket, demandes) = demarre(move |methode, params| {
        let Ok(mut garde) = etat.lock() else {
            return Reponse::Resultat(json!({}));
        };
        let (etat, envoye) = &mut *garde;
        match methode {
            "hello" => Reponse::Resultat(hello(1)),
            "terminal.run" => {
                if etat.nom_pris && params.get("name").is_some() {
                    return Reponse::Erreur {
                        message: String::from("name taken"),
                        kind: String::from("name_taken"),
                    };
                }
                Reponse::Resultat(json!({"address":"t9","foreground":{"pid":etat.pid_avant}}))
            }
            "terminal.read" => Reponse::Resultat(json!({"text": etat.ecran})),
            "terminal.send" => {
                *envoye = true;
                Reponse::Resultat(json!({"turn":0,"status":null,"started":null}))
            }
            "terminal.list" => {
                let pid = match (*envoye, etat.pid_apres) {
                    (true, Some(pid)) => pid,
                    _ => etat.pid_avant,
                };
                Reponse::Resultat(json!({"terminals":[{
                    "address":"t9","argv":[],"cwd":"/p","state": if etat.fini { "finished" } else { "live" },
                    "status":null,"turn":0,"title":"","labels":{},
                    "foreground":{"pid":pid,"name":"zsh","cwd":"/p"}}]}))
            }
            _ => Reponse::Resultat(json!({})),
        }
    });
    let port = PortCore::nouveau(
        ClientCore::nouveau(&socket),
        dossier_temporaire("l").join("x"),
    )
    .avec_delai_amorcage(Duration::from_millis(600));
    (port, demandes)
}

fn etat() -> Etat {
    Etat {
        ecran: String::from("~/p % "),
        pid_avant: 100,
        pid_apres: Some(200),
        fini: false,
        nom_pris: false,
    }
}

fn cree(port: &PortCore, espace: &str) -> Result<domain::ports::CreatedTab, PortError> {
    attends(port.create_tab(CreateTab {
        workspace: WorkspaceId::from(espace.to_string()),
        cwd: String::from("/p"),
        label: Some(String::from("Atelier 1")),
    }))
}

fn demarre_pi(port: &PortCore, pane: &str) -> Result<StartEvidence, PortError> {
    attends(port.start_agent(StartAgent {
        pane: PaneId::from(pane.to_string()),
        kind: AgentKind::from(String::from("pi")),
        name: None,
        args: vec![String::from("--model"), String::from("claude sonnet")],
    }))
}

#[test]
fn l_onglet_est_le_shell_de_la_machine_avec_les_labels_de_herdr_app() {
    let (port, demandes) = faux_core(etat());
    let cree = cree(&port, "sp1").unwrap_or_else(|erreur| panic!("{erreur}"));
    assert_eq!(cree.pane.to_string(), "t9");
    let onglet = cree.tab.to_string();
    assert!(onglet.starts_with("tb") && onglet.len() == 34, "{onglet}");
    let run = &demandes_de(&demandes, "terminal.run")[0];
    assert!(
        run.get("argv").is_none() && run.get("line").is_none(),
        "le shell de la machine"
    );
    assert_eq!(run["cwd"], "/p");
    assert_eq!(run["labels"]["herdr.space"], "sp1");
    assert_eq!(run["labels"]["herdr.tab"], Value::String(onglet));
    assert_eq!(run["no_parent"], true);
    assert_eq!(run["no_notify"], true);
    assert_eq!(run["name"], "atelier-1");
}

#[test]
fn un_nom_pris_n_empeche_pas_l_onglet() {
    let (port, demandes) = faux_core(Etat {
        nom_pris: true,
        ..etat()
    });
    assert!(cree(&port, "sp1").is_ok());
    let runs = demandes_de(&demandes, "terminal.run");
    assert_eq!(runs.len(), 2);
    assert!(runs[1].get("name").is_none());
}

#[test]
fn hors_d_un_espace_de_herdr_app_le_lancement_est_refuse() {
    let (port, demandes) = faux_core(etat());
    for espace in ["hors:/p", "herdr-app-layout-unavailable"] {
        match cree(&port, espace) {
            Err(PortError::Unsupported { feature }) => assert!(feature.contains("outside")),
            autre => panic!("{espace} : {autre:?}"),
        }
    }
    assert!(demandes_de(&demandes, "terminal.run").is_empty());
}

#[test]
fn l_agent_est_tape_dans_le_shell_cree_et_son_amorcage_observe() {
    let (port, demandes) = faux_core(etat());
    let cree = cree(&port, "sp1").unwrap_or_else(|erreur| panic!("{erreur}"));
    match demarre_pi(&port, &cree.pane.to_string()) {
        Ok(StartEvidence::Started { pane }) => assert_eq!(pane.to_string(), "t9"),
        autre => panic!("{autre:?}"),
    }
    let envoi = &demandes_de(&demandes, "terminal.send")[0];
    assert_eq!(envoi["terminal"], "t9");
    assert_eq!(envoi["text"], "pi --model 'claude sonnet'");
    assert!(envoi.get("force").is_none());
}

#[test]
fn jamais_dans_un_terminal_qu_on_n_a_pas_cree() {
    let (port, demandes) = faux_core(etat());
    match demarre_pi(&port, "t1") {
        Ok(StartEvidence::Failed { detail }) => assert!(detail.contains("not created"), "{detail}"),
        autre => panic!("{autre:?}"),
    }
    match attends(port.fermer_onglet(TabId::from(String::from("tb-de-l-utilisateur")))) {
        Err(PortError::Unsupported { feature }) => assert!(feature.contains("did not create")),
        autre => panic!("{autre:?}"),
    }
    assert!(demandes_de(&demandes, "terminal.send").is_empty());
    assert!(demandes_de(&demandes, "terminal.kill").is_empty());
}

#[test]
fn sans_invite_rien_n_est_tape_et_le_moteur_peut_rejouer() {
    let (port, demandes) = faux_core(Etat {
        ecran: String::from("  \n "),
        ..etat()
    });
    let cree = cree(&port, "sp1").unwrap_or_else(|erreur| panic!("{erreur}"));
    match demarre_pi(&port, &cree.pane.to_string()) {
        Ok(StartEvidence::ShellNotReady { .. }) => {}
        autre => panic!("{autre:?}"),
    }
    assert!(demandes_de(&demandes, "terminal.send").is_empty());
}

#[test]
fn rien_vu_dans_le_delai_est_ambigu_jamais_un_echec_rejoue() {
    let (port, _) = faux_core(Etat {
        pid_apres: None,
        ..etat()
    });
    let cree = cree(&port, "sp1").unwrap_or_else(|erreur| panic!("{erreur}"));
    match demarre_pi(&port, &cree.pane.to_string()) {
        Err(PortError::Timeout) => {}
        autre => panic!("{autre:?}"),
    }
}

#[test]
fn un_shell_termine_est_un_echec() {
    let (port, _) = faux_core(Etat {
        fini: true,
        pid_apres: None,
        ..etat()
    });
    let cree = cree(&port, "sp1").unwrap_or_else(|erreur| panic!("{erreur}"));
    match demarre_pi(&port, &cree.pane.to_string()) {
        Ok(StartEvidence::Failed { detail }) => assert!(detail.contains("exited"), "{detail}"),
        autre => panic!("{autre:?}"),
    }
}

#[test]
fn fermer_l_onglet_cree_termine_son_shell_une_seule_fois() {
    let (port, demandes) = faux_core(etat());
    let cree = cree(&port, "sp1").unwrap_or_else(|erreur| panic!("{erreur}"));
    if let Err(erreur) = attends(port.fermer_onglet(cree.tab.clone())) {
        panic!("{erreur}");
    }
    assert_eq!(demandes_de(&demandes, "terminal.kill")[0]["terminal"], "t9");
    assert!(attends(port.fermer_onglet(cree.tab)).is_err(), "déjà fermé");
}

#[test]
fn la_ligne_et_le_nom_sont_surs() {
    assert_eq!(ligne_de_lancement("cursor", &[]), "cursor-agent");
    assert_eq!(
        ligne_de_lancement(
            "pi",
            &[String::from("--x"), String::from("l'été; rm -rf /")]
        ),
        "pi --x 'l'\\''été; rm -rf /'"
    );
    assert_eq!(
        nom_de_terminal("Mon Projet/1").as_deref(),
        Some("mon-projet-1")
    );
    assert_eq!(nom_de_terminal("???"), None);
    assert_eq!(nom_de_terminal(&"a".repeat(40)).map(|n| n.len()), Some(32));
}
