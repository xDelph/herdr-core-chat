use herdr_core_client::disposition::{Disposition, disposition_depuis, terminaux_de_root};
use serde_json::json;

#[test]
fn parse_disposition_reelle() {
    let doc = json!({
        "spaces": [
            {"id":"sp1","name":"","folder":"/home/u/proj","tabs":[
                {"id":"tb1","name":"","root":{"type":"row","children":[{"type":"block","content":{"terminal":"t3"}}]}},
                {"id":"tb2","name":"","root":{"type":"row","children":[{"type":"block","content":{"terminal":"t7"}}]}}
            ]}
        ]
    });
    let d = disposition_depuis(&doc).unwrap();
    assert_eq!(d.spaces.len(), 1);
    assert_eq!(d.spaces[0].id, "sp1");
    assert_eq!(d.spaces[0].folder, "/home/u/proj");
    assert_eq!(d.spaces[0].onglets.len(), 2);
    assert_eq!(d.spaces[0].onglets[0].terminaux, vec!["t3"]);
    assert_eq!(d.spaces[0].onglets[1].terminaux, vec!["t7"]);
}

#[test]
fn parse_imbrique_children() {
    let root = json!({"type":"col","children":[
        {"type":"row","children":[{"type":"block","content":{"terminal":"t1"}},{"type":"block","content":{"terminal":"t2"}}]}
    ]});
    assert_eq!(terminaux_de_root(&root), vec!["t1", "t2"]);
}

#[test]
fn format_inconnu_renvoie_erreur() {
    let doc = json!({"foo": 1});
    assert!(disposition_depuis(&doc).is_err());
    let doc2 = json!({"spaces": "pas un tableau"});
    assert!(disposition_depuis(&doc2).is_err());
    let doc3 = json!({"spaces": [{"id":"sp1"}]}); // tabs manquant
    assert!(disposition_depuis(&doc3).is_err());
}

#[test]
fn disposition_vide_est_valide() {
    let doc = json!({"spaces": []});
    let d = disposition_depuis(&doc).unwrap();
    assert!(d.spaces.is_empty());
}
