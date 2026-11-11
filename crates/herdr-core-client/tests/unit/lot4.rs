//! Lot 4 : le flux d'événements, les records 7501, l'épingle, l'écran
//! inexact et la pièce jointe — chaque forme est celle RELEVÉE sur un core
//! isolé herdr-core 0.1.19 (2026-11-10), pas celle supposée.

use domain::status::AgentStatus;
use herdr_core_client::abonnement::Pertinence;
use herdr_core_client::client::{Terminal, base64_de};
use herdr_core_client::port::PortCore;
use serde_json::json;

fn evenement(genre: &str, terminal: &str, data: serde_json::Value) -> serde_json::Value {
    json!({"jsonrpc":"2.0","method":"event","params":{"type":genre,"terminal":terminal,"seq":1,"data":data}})
}

#[test]
fn seuls_les_changements_qui_comptent_reveillent_la_veille() {
    let mut juge = Pertinence::default();
    juge.instantane(&[json!({
        "address":"t1","title":"✳ Claude Code",
        "status":{"app":"claude-code","state":"working","msg":"Reading"},"records":{}
    })]);
    // Le message d'activité change à chaque outil : pas de relecture.
    assert!(!juge.juge(&evenement("status","t1",json!({
        "records":{},"reported":true,"status":{"app":"claude-code","state":"working","msg":"Editing"}
    }))));
    // Le mot change : relire.
    assert!(juge.juge(&evenement("status","t1",json!({
        "records":{},"reported":true,"status":{"app":"claude-code","state":"blocked","kind":"permission"}
    }))));
    // La nature de l'attente change seule : relire aussi.
    assert!(juge.juge(&evenement("status","t1",json!({
        "records":{},"reported":true,"status":{"app":"claude-code","state":"blocked","kind":"question"}
    }))));
    // Un sous-agent naît, puis change d'état : relire chaque fois.
    assert!(juge.juge(&evenement("status","t1",json!({
        "records":{"agent-abc":{"app":"fake","state":"working","title":"Explore repo","msg":"reading"}},
        "status":{"app":"claude-code","state":"blocked","kind":"question"}
    }))));
    assert!(!juge.juge(&evenement("status","t1",json!({
        "records":{"agent-abc":{"app":"fake","state":"working","title":"Explore repo","msg":"writing"}},
        "status":{"app":"claude-code","state":"blocked","kind":"question"}
    }))), "le msg d'un record ne réveille pas");
    assert!(juge.juge(&evenement(
        "status",
        "t1",
        json!({
            "records":{"agent-abc":{"app":"fake","state":"done","title":"Explore repo"}},
            "status":{"app":"claude-code","state":"blocked","kind":"question"}
        })
    )));
    // Le spinner du titre tourne : rien ; le texte change : relire.
    assert!(!juge.juge(&evenement("title", "t1", json!({"title":"⠐ Claude Code"}))));
    assert!(juge.juge(&evenement(
        "title",
        "t1",
        json!({"title":"⠐ Fix the login"})
    )));
    // Taille : rien. Tour, fin, label, création : relire.
    assert!(!juge.juge(&evenement("size", "t1", json!({"cols":80,"rows":24}))));
    for genre in [
        "turn",
        "finished",
        "labels",
        "created",
        "lost",
        "restarted",
        "doc",
    ] {
        assert!(
            juge.juge(&evenement(genre, "t1", json!({}))),
            "{genre} réveille"
        );
    }
    // Le battement ne réveille pas ; un `missed` (ou tout inconnu), si.
    assert!(!juge.juge(&json!({"jsonrpc":"2.0","method":"heartbeat","params":{"seq":110}})));
    assert!(
        juge.juge(&json!({"jsonrpc":"2.0","method":"missed","params":{"seq":200,"terminals":[]}}))
    );
}

fn terminal() -> Terminal {
    Terminal {
        screen: None,
        records: Default::default(),
        address: String::from("t1"),
        name: None,
        argv: vec![String::from("claude")],
        cwd: String::from("/projets/x"),
        state: String::from("live"),
        status: Some(json!({"app":"claude-code","state":"working"})),
        turn: 3,
        status_ms: Some(1),
        title: String::new(),
        labels: std::collections::BTreeMap::new(),
        parent: None,
        foreground_pid: None,
        foreground_cwd: None,
        foreground_name: None,
        foreground_argv: Vec::new(),
        exit_code: None,
        reason: None,
        signal: None,
    }
}

#[test]
fn les_records_l_epingle_et_l_ecran_inexact_sont_lus_tels_quels() {
    let mut term = terminal();
    term.records.insert(
        String::from("agent-abc"),
        json!({"app":"fake","state":"working","title":"Explore repo","msg":"reading files"}),
    );
    term.records.insert(
        String::from("agent-zzz"),
        json!({"app":"fake","state":"done"}),
    );
    // Un record sans mot connu n'est pas deviné.
    term.records.insert(
        String::from("bizarre"),
        json!({"app":"fake","state":"dancing"}),
    );
    term.labels
        .insert(String::from("herdr.pinned"), String::from("1"));
    term.screen = Some(String::from("inexact"));
    let faits = PortCore::faits_de(&term);
    assert_eq!(faits.sous_agents.len(), 2);
    assert_eq!(faits.sous_agents[0].id, "agent-abc");
    assert_eq!(faits.sous_agents[0].status, AgentStatus::Working);
    assert_eq!(faits.sous_agents[0].title.as_deref(), Some("Explore repo"));
    assert_eq!(faits.sous_agents[0].msg.as_deref(), Some("reading files"));
    assert_eq!(faits.sous_agents[1].status, AgentStatus::Done);
    assert!(faits.epingle);
    assert!(faits.ecran_inexact);

    // Une épingle retirée (`herdr.pinned=` vide), un écran exact : rien.
    let mut sans = terminal();
    sans.labels
        .insert(String::from("herdr.pinned"), String::new());
    sans.screen = Some(String::from("exact"));
    let faits = PortCore::faits_de(&sans);
    assert!(!faits.epingle && !faits.ecran_inexact && faits.sous_agents.is_empty());
}

#[test]
fn le_base64_de_put_est_le_standard() {
    assert_eq!(base64_de(b""), "");
    assert_eq!(base64_de(b"f"), "Zg==");
    assert_eq!(base64_de(b"fo"), "Zm8=");
    assert_eq!(base64_de(b"foo"), "Zm9v");
    assert_eq!(base64_de(b"hello world"), "aGVsbG8gd29ybGQ=");
    assert_eq!(base64_de(&[0xff, 0xfe, 0x00]), "//4A");
}
