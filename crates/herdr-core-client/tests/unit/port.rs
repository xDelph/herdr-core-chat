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
    // Cursor tourne sous `node` : reconnu par l'argv du premier plan.
    let mut cursor = terminal("t4", Value::Null, "node", "/p");
    cursor["foreground"]["argv"] = json!(["/Users/moi/.local/bin/cursor-agent", "index.js"]);
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
        cursor,
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
            (
                String::from("t4"),
                Some(String::from("cursor")),
                AgentStatus::Unknown
            ),
        ]
    );
    assert_eq!(snapshot.panes.len(), 4, "le shell reste un pane visible");
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

/// `terminal.send` n'attend le démarrage que ≈5 s : au-delà, `started` vaut
/// `false` alors que l'agent a pu prendre le message. L'adaptateur prolonge
/// l'observation — `working` ou un tour qui avance prouvent le démarrage ;
/// un agent resté `idle` au même tour reste « non confirmé ».
#[test]
fn un_demarrage_tardif_est_confirme_par_le_statut_ou_le_tour() {
    for (etat, tour, confirme) in [("working", 4, true), ("idle", 5, true), ("idle", 4, false)] {
        let mut term = terminal("t1", json!({"app":"faux","state":etat}), "faux", "/p");
        term["turn"] = json!(tour);
        let (port, demandes) = port_sur(json!([term]), sans_doc(), |_, _| {
            Reponse::Resultat(json!({"turn": 4, "status": "idle", "started": false}))
        });
        let port = port.avec_delai_confirmation(std::time::Duration::from_millis(300));
        match prompt(&port) {
            PromptEvidence::Confirmed { .. } => {
                assert!(confirme, "{etat} au tour {tour} : confirmé sans preuve");
            }
            PromptEvidence::Unknown { detail, .. } => {
                assert!(!confirme, "{etat} au tour {tour} : démarrage manqué");
                assert!(detail.contains("did not start working"), "{detail}");
            }
            autre => panic!("{autre:?}"),
        }
        assert!(
            !demandes_de(&demandes, "terminal.list").is_empty(),
            "le terminal doit être relu après la fenêtre de terminal.send"
        );
    }
}

/// `started: true` suffit : aucune relecture, aucune attente de plus.
#[test]
fn un_demarrage_vu_par_le_core_ne_relit_pas_le_terminal() {
    let (port, demandes) = port_sur(json!([]), sans_doc(), |_, _| {
        Reponse::Resultat(json!({"turn": 4, "status": "idle", "started": true}))
    });
    assert!(matches!(prompt(&port), PromptEvidence::Confirmed { .. }));
    assert!(demandes_de(&demandes, "terminal.list").is_empty());
}

#[test]
fn un_agent_bloque_refuse_le_prompt_et_la_cause_remonte() {
    let (port, _) = port_sur(json!([]), sans_doc(), |_, _| Reponse::Erreur {
        message: String::from("t1 is blocked (permission)"),
        kind: String::from("blocked"),
    });
    // L'erreur TYPÉE (`Upstream`, état `erreur` côté PWA), jamais un
    // `Failed` que l'application range en « inconnu ».
    match attends(port.prompt(TargetedPrompt {
        pane: PaneId::from(String::from("t1")),
        literal_text: String::from("bonjour"),
        fingerprint: DeliveryFingerprint::from(String::from("e")),
    })) {
        Err(PortError::Upstream { code, message }) => {
            assert_eq!(code, "blocked");
            assert!(message.contains("permission"), "{message}");
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

/// Un agent `done`, son tour, le tour vu (`herdr.seen`) et la fin du tour.
fn fini(tour: u64, vu: Option<&str>, fin_ms: u64) -> Value {
    let mut term = terminal(
        "t1",
        json!({"app":"claude-code","state":"done"}),
        "claude",
        "/p",
    );
    term["turn"] = json!(tour);
    term["status_ms"] = json!(fin_ms);
    if let Some(vu) = vu {
        term["labels"] = json!({"herdr.seen": vu});
    }
    term
}

/// Comme Herdr.app : `done` regardé (`herdr.seen >= turn`) s'affiche `idle`,
/// et dit jusqu'où il a été vu ; un tour pas encore vu reste `done`.
#[test]
fn un_done_vu_dans_herdr_app_devient_idle_et_porte_son_instant() {
    for (vu, statut, vu_a) in [
        (Some("7"), AgentStatus::Idle, Some(1_000)),
        (Some("8"), AgentStatus::Idle, Some(1_000)),
        (Some("6"), AgentStatus::Done, None),
        (Some("pas un tour"), AgentStatus::Done, None),
        (None, AgentStatus::Done, None),
    ] {
        let (port, _) = port_sur(json!([fini(7, vu, 1_000)]), sans_doc(), rien);
        let agent = &snapshot(&port).agents[0];
        assert_eq!(agent.status, statut, "herdr.seen = {vu:?}");
        assert_eq!(agent.seen_at, vu_a, "herdr.seen = {vu:?}");
    }
}

/// Un agent au travail n'a jamais de « vu » : son tour n'est pas fini.
#[test]
fn un_agent_au_travail_n_est_jamais_vu() {
    let mut term = terminal(
        "t1",
        json!({"app":"claude-code","state":"working"}),
        "claude",
        "/p",
    );
    term["labels"] = json!({"herdr.seen": "3"});
    term["status_ms"] = json!(1_000);
    let (port, _) = port_sur(json!([term]), sans_doc(), rien);
    let agent = &snapshot(&port).agents[0];
    assert_eq!(agent.status, AgentStatus::Working);
    assert_eq!(agent.seen_at, None);
}

fn marque_vu(port: &PortCore, vu_a: u64) -> Option<u64> {
    attends(port.marque_vu(PaneId::from(String::from("t1")), vu_a))
        .unwrap_or_else(|erreur| panic!("marque_vu : {erreur}"))
}

/// Lu sur le téléphone → `herdr.seen = turn`, seulement pour un `done`
/// fini AVANT la lecture et pas encore vu. Jamais un tour lu après coup,
/// jamais un label en arrière, jamais un agent au travail.
#[test]
fn une_lecture_note_le_vu_dans_herdr_app_sans_jamais_l_inventer() {
    for (term, vu_a, attendu) in [
        (fini(7, Some("6"), 1_000), 1_500, Some(7)),
        (fini(7, None, 1_000), 1_000, Some(7)),
        (fini(7, Some("6"), 2_000), 1_500, None),
        (fini(7, Some("7"), 1_000), 1_500, None),
        (fini(7, Some("9"), 1_000), 1_500, None),
        (
            terminal(
                "t1",
                json!({"app":"claude-code","state":"working"}),
                "claude",
                "/p",
            ),
            9_999_999,
            None,
        ),
    ] {
        let (port, demandes) = port_sur(json!([term.clone()]), sans_doc(), rien);
        assert_eq!(marque_vu(&port, vu_a), attendu, "{term}");
        let poses = demandes_de(&demandes, "terminal.label");
        match attendu {
            Some(tour) => {
                assert_eq!(poses.len(), 1);
                assert_eq!(poses[0]["terminal"], "t1");
                assert_eq!(poses[0]["labels"], json!({"herdr.seen": tour.to_string()}));
            }
            None => assert!(poses.is_empty(), "label posé sans preuve : {poses:?}"),
        }
    }
}

/// Un refus du core remonte avec sa cause.
#[test]
fn un_vu_refuse_par_le_core_remonte() {
    let (port, _) = port_sur(json!([fini(7, None, 1_000)]), sans_doc(), |_, _| {
        Reponse::Erreur {
            message: String::from("terminal t1 not found"),
            kind: String::from("not_found"),
        }
    });
    match attends(port.marque_vu(PaneId::from(String::from("t1")), 2_000)) {
        Err(erreur) => assert!(erreur.description().contains("not_found"), "{erreur}"),
        Ok(issue) => panic!("refus masqué : {issue:?}"),
    }
}

/// Un collage refusé par le core (permission en attente) est une **erreur** :
/// l'appelant qui enchaîne `enter` s'arrête — un `Ok(Refused)` était suivi
/// d'un vrai `enter` qui approuvait la permission.
#[test]
fn un_collage_ou_des_touches_refuses_sont_des_erreurs() {
    let (port, demandes) = port_sur(
        json!([terminal(
            "t1",
            json!({"app":"claude-code","state":"blocked","kind":"permission"}),
            "claude",
            "/p"
        )]),
        sans_doc(),
        |_, _| Reponse::Erreur {
            message: String::from("terminal t1 is blocked"),
            kind: String::from("blocked"),
        },
    );
    match attends(port.send_text(TargetedText {
        pane: PaneId::from(String::from("t1")),
        text: String::from("non, fais plutôt X"),
    })) {
        Err(PortError::Upstream { code, .. }) => assert_eq!(code, "blocked"),
        autre => panic!("refus masqué : {autre:?}"),
    }
    // Rien ne marque « entrée déjà appuyée » : un `enter` qui suivrait
    // partirait vraiment — et serait lui aussi refusé ici.
    match attends(port.send_keys(TargetedKeys {
        pane: PaneId::from(String::from("t1")),
        keys: vec![String::from("enter")],
    })) {
        Err(PortError::Upstream { code, .. }) => assert_eq!(code, "blocked"),
        autre => panic!("refus masqué : {autre:?}"),
    }
    assert_eq!(demandes_de(&demandes, "terminal.keys").len(), 1);
}

/// Un onglet lancé ICI (labels `herdr.space`/`herdr.tab`) que Herdr.app n'a
/// pas encore mis dans son doc : il est dans SON espace, dans SON onglet —
/// jamais « (outside Herdr.app) ». Et l'espace porte son dossier déclaré.
#[test]
fn un_onglet_lance_ici_est_dans_l_espace_de_ses_labels() {
    let mut lance = terminal("t9", json!({"app":"pi","state":"idle"}), "pi", "/p/sous");
    lance["labels"] = json!({"herdr.space": "sp1", "herdr.tab": "tbneuf"});
    lance["name"] = json!("p-1");
    let mut etranger = terminal("t8", json!({"app":"pi","state":"idle"}), "pi", "/x");
    // Un label vers un espace inconnu ne vaut rien : dehors, par dossier.
    etranger["labels"] = json!({"herdr.space": "sp-inconnu", "herdr.tab": "tb"});
    let doc = json!({"version": 3, "value": {"spaces": [{"id":"sp1","name":"","folder":"/p","tabs":[]}]}});
    let (port, _) = port_sur(json!([lance, etranger]), doc, rien);
    let snapshot = snapshot(&port);
    let agent = |id: &str| {
        snapshot
            .agents
            .iter()
            .find(|a| a.terminal_id == id)
            .map(|a| (a.workspace_id.to_string(), a.tab_id.to_string()))
    };
    assert_eq!(
        agent("t9"),
        Some((String::from("sp1"), String::from("tbneuf")))
    );
    assert_eq!(
        agent("t8").map(|(espace, _)| espace),
        Some(String::from("hors:/x"))
    );
    let onglet = snapshot
        .tabs
        .unwrap_or_default()
        .into_iter()
        .find(|t| t.tab_id.to_string() == "tbneuf");
    assert_eq!(onglet.and_then(|t| t.label).as_deref(), Some("p-1"));
    let espace = snapshot
        .workspaces
        .iter()
        .find(|w| w.workspace_id.to_string() == "sp1");
    assert_eq!(espace.and_then(|w| w.folder.clone()).as_deref(), Some("/p"));
}
