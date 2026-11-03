//! Lecture tolérante du doc `herdr-app` : structure connue, cause nommée
//! sinon.

use herdr_core_client::disposition::{disposition_depuis, terminaux_de_root};
use serde_json::json;

#[test]
fn les_splits_imbriques_donnent_tous_leurs_terminaux() {
    let root = json!({"type":"col","children":[
        {"type":"row","children":[{"type":"block","content":{"terminal":"t1"}},
                                  {"type":"block","content":{"terminal":"t2"}}]}
    ]});
    assert_eq!(terminaux_de_root(&root), vec!["t1", "t2"]);
}

#[test]
fn un_format_inconnu_est_une_erreur_nommee() {
    for doc in [
        json!({"foo": 1}),
        json!({"spaces": "pas un tableau"}),
        json!({"spaces": [{"id":"sp1"}]}),
        json!({"spaces": [{"tabs": []}]}),
    ] {
        match disposition_depuis(&doc) {
            Err(cause) => assert!(!cause.detail.is_empty()),
            Ok(lue) => panic!("format inconnu accepté : {lue:?}"),
        }
    }
}

#[test]
fn une_disposition_vide_est_valide() {
    match disposition_depuis(&json!({"spaces": []})) {
        Ok(lue) => assert!(lue.spaces.is_empty()),
        Err(cause) => panic!("{cause}"),
    }
}
