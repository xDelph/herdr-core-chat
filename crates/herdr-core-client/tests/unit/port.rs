//! L'adaptateur `ControlPort` sur un faux core : statuts, disposition,
//! preuves d'envoi, arrêt — et les refus dits tels quels.

use super::faux_core::{
    Reponse, attends, demandes_de, demarre, dossier_temporaire, hello, terminal,
};
use domain::errors::PortError;
use domain::ids::{DeliveryFingerprint, PaneId};
use domain::ports::{
    ControlPort, PartialStep, PromptEvidence, StopEvidence, TargetedKeys, TargetedPrompt,
    TargetedText, TransportOutcome,
};
use domain::status::AgentStatus;
use herdr_core_client::ClientCore;
use herdr_core_client::port::PortCore;
use serde_json::{Value, json};

/// Un faux core qui sert `terminals` et le doc donné ; les autres méthodes
/// répondent par `autre`.
fn port_sur<F>(terminals: Value, doc: Value, autre: F) -> (PortCore, super::faux_core::Demandes)
where
    F: Fn(&str, &Value) -> Reponse + Send + 'static,
{
    let (socket, demandes) = demarre(move |methode, params| match methode {
        "hello" => Reponse::Resultat(hello(1)),
        "terminal.list" => Reponse::Resultat(json!({"terminals": terminals.clone()})),
        "doc.get" => Reponse::Resultat(doc.clone()),
        _ => autre(methode, params),
    });
    let etat = dossier_temporaire("etat").join("absent.json");
    (
        PortCore::nouveau(ClientCore::nouveau(&socket), etat),
        demandes,
    )
}

fn sans_doc() -> Value {
    json!({"value": null, "version": 0})
}

fn rien(_: &str, _: &Value) -> Reponse {
    Reponse::Resultat(json!({}))
}

fn snapshot(port: &PortCore) -> domain::observation::Snapshot {
    attends(port.snapshot()).unwrap_or_else(|erreur| panic!("snapshot : {erreur}"))
}

#[test]
fn chaque_mot_de_statut_a_son_etat_y_compris_error() {
    let terms = json!([
        terminal(
            "t1",
            json!({"app":"claude-code","state":"idle"}),
            "claude",
            "/p"
        ),
        terminal(
            "t2",
            json!({"app":"claude-code","state":"working"}),
            "claude",
            "/p"
        ),
        terminal(
            "t3",
            json!({"app":"opencode","state":"blocked","kind":"permission"}),
            "opencode",
            "/p"
        ),
        terminal(
            "t4",
            json!({"app":"opencode","state":"done"}),
            "opencode",
            "/p"
        ),
        terminal(
            "t5",
            json!({"app":"opencode","state":"error"}),
            "opencode",
            "/p"
        ),
        terminal(
            "t6",
            json!({"app":"opencode","state":"rêveur"}),
            "opencode",
            "/p"
        ),
    ]);
    let (port, _) = port_sur(terms, sans_doc(), rien);
    let etats: Vec<AgentStatus> = snapshot(&port).agents.iter().map(|a| a.status).collect();
    assert_eq!(
        etats,
        [
            AgentStatus::Idle,
            AgentStatus::Working,
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Error,
            AgentStatus::Unknown,
        ]
    );
}

#[test]
fn le_type_vient_du_programme_jamais_du_shell() {
    let terms = json!([
        terminal(
            "t1",
            json!({"app":"claude-code","state":"idle"}),
            "claude",
            "/p"
        ),
        // Pi sans statut 7501 : reconnu par le premier plan, statut inconnu.
        terminal("t2", Value::Null, "pi", "/p"),
        // Un shell nu n'est pas un agent (argv = /bin/zsh).
        terminal("t3", Value::Null, "zsh", "/p"),
    ]);
    let (port, _) = port_sur(terms, sans_doc(), rien);
    let snapshot = snapshot(&port);
    let agents: Vec<(String, Option<String>, AgentStatus)> = snapshot
        .agents
        .iter()
        .map(|a| {
            (
                a.terminal_id.clone(),
                a.agent.as_ref().map(ToString::to_string),
                a.status,
            )
        })
        .collect();
    assert_eq!(
        agents,
        [
            (
                String::from("t1"),
                Some(String::from("claude")),
                AgentStatus::Idle
            ),
            (
                String::from("t2"),
                Some(String::from("pi")),
                AgentStatus::Unknown
            ),
        ]
    );
    assert_eq!(snapshot.panes.len(), 3, "le shell reste un pane visible");
}

#[test]
fn une_generation_inconnue_est_refusee() {
    let (socket, _) = demarre(|methode, _| match methode {
        "hello" => Reponse::Resultat(hello(2)),
        _ => Reponse::Resultat(json!({"terminals": []})),
    });
    let port = PortCore::nouveau(
        ClientCore::nouveau(&socket),
        dossier_temporaire("e").join("x"),
    );
    match attends(port.snapshot()) {
        Err(PortError::Unsupported { feature }) => {
            assert!(feature.contains("generation 2"), "{feature}")
        }
        autre => panic!("génération 2 acceptée : {autre:?}"),
    }
    match attends(port.ping()) {
        Err(PortError::Unsupported { .. }) => {}
        autre => panic!("ping génération 2 : {autre:?}"),
    }
}

#[test]
fn une_disposition_illisible_est_affichee_avec_sa_cause() {
    let terms = json!([terminal(
        "t1",
        json!({"app":"claude-code","state":"idle"}),
        "claude",
        "/p"
    )]);
    let doc = json!({"value": {"spaces": "pas un tableau"}, "version": 41});
    let (port, _) = port_sur(terms, doc, rien);
    let snapshot = snapshot(&port);
    assert_eq!(snapshot.workspaces.len(), 1);
    let label = snapshot.workspaces[0].label.clone().unwrap_or_default();
    assert!(
        label.starts_with("Herdr.app layout unavailable: "),
        "{label}"
    );
    assert_eq!(
        label,
        "Herdr.app layout unavailable: doc `herdr-app` v41: `spaces` is not an array"
    );
    assert_eq!(snapshot.agents.len(), 1, "les agents restent visibles");
}

#[test]
fn hors_disposition_l_enfant_suit_son_parent_l_orphelin_son_dossier() {
    let mut enfant = terminal(
        "t2",
        json!({"app":"claude-code","state":"working"}),
        "claude",
        "/p/sous",
    );
    enfant["parent"] = json!("t1");
    let terms = json!([
        terminal(
            "t1",
            json!({"app":"claude-code","state":"idle"}),
            "claude",
            "/p"
        ),
        enfant,
        terminal(
            "t3",
            json!({"app":"opencode","state":"idle"}),
            "opencode",
            "/ailleurs/outil"
        ),
    ]);
    let doc = json!({"version": 3, "value": {"spaces": [{"id":"sp1","name":"","folder":"/p","tabs":[
        {"id":"tb1","name":"","root":{"type":"row","children":[{"type":"block","content":{"terminal":"t1"}}]}}
    ]}]}});
    let (port, _) = port_sur(terms, doc, rien);
    let snapshot = snapshot(&port);
    let place: Vec<(String, String, String)> = snapshot
        .agents
        .iter()
        .map(|a| {
            (
                a.terminal_id.clone(),
                a.workspace_id.to_string(),
                a.tab_id.to_string(),
            )
        })
        .collect();
    assert_eq!(
        place[0],
        (String::from("t1"), String::from("sp1"), String::from("tb1"))
    );
    assert_eq!(
        place[1].1, "sp1",
        "l'enfant est dans l'espace de son parent"
    );
    assert_ne!(place[1].2, "tb1", "dans un onglet à part");
    assert_eq!(place[2].1, "hors:/ailleurs/outil");
    let labels: Vec<Option<String>> = snapshot
        .workspaces
        .iter()
        .map(|w| w.label.clone())
        .collect();
    assert_eq!(labels[1].as_deref(), Some("outil (outside Herdr.app)"));
    // Sans état Herdr.app : le nom du dossier de l'espace, pas son chemin.
    assert_eq!(labels[0].as_deref(), Some("p"));
    // L'espace agrège ce qui demande le plus : `working` (l'enfant) > `idle`.
    assert_eq!(
        snapshot.workspaces[0].agent_status,
        Some(AgentStatus::Working)
    );
    let onglets = snapshot.tabs.unwrap_or_default();
    let dehors = onglets.iter().find(|t| t.tab_id.to_string() == place[1].2);
    assert_eq!(
        dehors.and_then(|t| t.label.clone()).as_deref(),
        Some("Outside Herdr.app")
    );
}

fn prompt(port: &PortCore) -> PromptEvidence {
    attends(port.prompt(TargetedPrompt {
        pane: PaneId::from(String::from("t1")),
        literal_text: String::from("fais-le"),
        fingerprint: DeliveryFingerprint::from(String::from("empreinte")),
    }))
    .unwrap_or_else(|erreur| panic!("prompt : {erreur}"))
}

#[test]
fn le_prompt_n_est_confirme_que_si_l_agent_demarre() {
    for (started, confirme) in [
        (json!(true), true),
        (json!(false), false),
        (Value::Null, false),
    ] {
        let reponse = json!({"turn": 4, "status": "idle", "started": started});
        let (port, demandes) = port_sur(json!([]), sans_doc(), move |_, _| {
            Reponse::Resultat(reponse.clone())
        });
        match prompt(&port) {
            PromptEvidence::Confirmed { .. } => assert!(confirme, "confirmé sans démarrage"),
            PromptEvidence::Unknown { partial, detail } => {
                assert!(!confirme, "démarrage non reconnu");
                assert_eq!(
                    partial,
                    [
                        PartialStep::LiteralTextWritten,
                        PartialStep::EnterSequenceSent
                    ]
                );
                assert!(detail.contains("turn 4"), "{detail}");
            }
            autre => panic!("{autre:?}"),
        }
        let envoyes = demandes_de(&demandes, "terminal.send");
        assert_eq!(envoyes.len(), 1);
        assert!(
            envoyes[0].get("force").is_none(),
            "un prompt ne force jamais"
        );
    }
}

#[test]
fn un_agent_bloque_refuse_le_prompt_et_la_cause_remonte() {
    let (port, _) = port_sur(json!([]), sans_doc(), |_, _| Reponse::Erreur {
        message: String::from("t1 is blocked (permission)"),
        kind: String::from("blocked"),
    });
    match prompt(&port) {
        PromptEvidence::Failed { detail } => {
            assert!(detail.contains("blocked"), "{detail}");
            assert!(detail.contains("permission"), "{detail}");
        }
        autre => panic!("refus masqué : {autre:?}"),
    }
}

fn texte(port: &PortCore) -> domain::ports::TransportEvidence {
    attends(port.send_text(TargetedText {
        pane: PaneId::from(String::from("t1")),
        text: String::from("ma réponse"),
    }))
    .unwrap_or_else(|erreur| panic!("send_text : {erreur}"))
}

fn touches(port: &PortCore, keys: &[&str]) -> domain::ports::TransportEvidence {
    attends(port.send_keys(TargetedKeys {
        pane: PaneId::from(String::from("t1")),
        keys: keys.iter().map(|k| (*k).to_string()).collect(),
    }))
    .unwrap_or_else(|erreur| panic!("send_keys : {erreur}"))
}

fn port_d_action(status: Value) -> (PortCore, super::faux_core::Demandes) {
    port_sur(
        json!([terminal("t1", status, "claude", "/p")]),
        sans_doc(),
        |methode, _| match methode {
            "terminal.send" => {
                Reponse::Resultat(json!({"turn": 2, "status": "blocked", "started": null}))
            }
            _ => Reponse::Resultat(json!({"turn": 2})),
        },
    )
}

#[test]
fn une_reponse_libre_force_une_question_jamais_une_permission() {
    let (port, demandes) =
        port_d_action(json!({"app":"claude-code","state":"blocked","kind":"question"}));
    let preuve = texte(&port);
    assert_eq!(preuve.outcome, TransportOutcome::Delivered);
    assert_eq!(
        preuve.partial,
        [
            PartialStep::LiteralTextWritten,
            PartialStep::EnterSequenceSent
        ]
    );
    assert_eq!(demandes_de(&demandes, "terminal.send")[0]["force"], true);

    let (port, demandes) =
        port_d_action(json!({"app":"claude-code","state":"blocked","kind":"permission"}));
    let _ = texte(&port);
    assert!(
        demandes_de(&demandes, "terminal.send")[0]
            .get("force")
            .is_none(),
        "une décision de permission n'est jamais forcée"
    );
}

#[test]
fn l_entree_qui_suit_un_texte_n_est_pas_appuyee_deux_fois() {
    let (port, demandes) =
        port_d_action(json!({"app":"claude-code","state":"blocked","kind":"question"}));
    let _ = texte(&port);
    let preuve = touches(&port, &["enter"]);
    assert_eq!(preuve.partial, [PartialStep::EnterSequenceSent]);
    assert!(
        demandes_de(&demandes, "terminal.keys").is_empty(),
        "second Entrée envoyé"
    );
    // Le suivant, lui, part bien : l'absorption ne vaut qu'une fois.
    let preuve = touches(&port, &["enter"]);
    assert_eq!(preuve.partial, [PartialStep::KeysSent]);
    assert_eq!(demandes_de(&demandes, "terminal.keys").len(), 1);
}

#[test]
fn l_arret_est_envoye_quel_que_soit_l_effet_observe() {
    for outcome in ["stopped", "exited", "still_working", "unknown"] {
        let (port, _) = port_sur(json!([]), sans_doc(), move |_, _| {
            Reponse::Resultat(json!({"outcome": outcome, "key": "esc"}))
        });
        match attends(port.stop(PaneId::from(String::from("t1")))) {
            Ok(StopEvidence::InterruptSent) => {}
            autre => panic!("{outcome} : {autre:?}"),
        }
    }
    let (port, _) = port_sur(json!([]), sans_doc(), |_, _| Reponse::Erreur {
        message: String::from("t9: no such terminal"),
        kind: String::from("not_found"),
    });
    match attends(port.stop(PaneId::from(String::from("t9")))) {
        Err(PortError::Upstream { code, message }) => {
            assert_eq!(code, "not_found");
            assert!(message.contains("t9"));
        }
        autre => panic!("{autre:?}"),
    }
}
