//! Le transport JSON-RPC : aller-retour, erreur amont avec `data.kind`.

use super::faux_core::{Reponse, demarre, hello};
use herdr_core_client::{ClientCore, ErreurTransport};
use serde_json::json;

#[test]
fn hello_et_terminal_list_sur_core_simule() {
    let (socket, _) = demarre(|methode, _| match methode {
        "hello" => Reponse::Resultat(hello(1)),
        _ => Reponse::Resultat(json!({"terminals":[{"address":"t1","argv":["sleep","10"],
            "cwd":"/tmp","state":"live","status":null,"turn":0,"title":"","labels":{}}]})),
    });
    let mut client = ClientCore::nouveau(&socket);
    match client.hello() {
        Ok(reponse) => assert_eq!(reponse.version, "0.1.17"),
        Err(erreur) => panic!("hello : {erreur}"),
    }
    match client.terminal_list() {
        Ok(liste) => {
            assert_eq!(liste.terminals.len(), 1);
            assert_eq!(liste.terminals[0].address, "t1");
        }
        Err(erreur) => panic!("terminal.list : {erreur}"),
    }
}

#[test]
fn erreur_amont_est_propagee_avec_son_kind() {
    let (socket, _) = demarre(|_, _| Reponse::Erreur {
        message: String::from("boom"),
        kind: String::from("not_found"),
    });
    let mut client = ClientCore::nouveau(&socket);
    match client.server_status() {
        Err(ErreurTransport::AmontRefus { kind, .. }) => {
            assert_eq!(kind.as_deref(), Some("not_found"))
        }
        autre => panic!("attendu AmontRefus, eu {autre:?}"),
    }
}

#[test]
fn le_statut_de_send_est_un_mot() {
    // `SendResult.status` est un MOT (`"working"`), pas l'objet 7501 : le
    // lire comme un objet le perdait toujours.
    let (socket, demandes) =
        demarre(|_, _| Reponse::Resultat(json!({"turn":7,"status":"working","started":true})));
    let mut client = ClientCore::nouveau(&socket);
    match client.terminal_send("t1", "salut", Some("idle"), false) {
        Ok(envoi) => {
            assert_eq!(envoi.turn, 7);
            assert_eq!(envoi.status.as_deref(), Some("working"));
            assert_eq!(envoi.started, Some(true));
        }
        Err(erreur) => panic!("terminal.send : {erreur}"),
    }
    let envoyes = super::faux_core::demandes_de(&demandes, "terminal.send");
    assert_eq!(envoyes[0]["if"], "idle");
    assert!(envoyes[0].get("force").is_none(), "jamais force par défaut");
}
