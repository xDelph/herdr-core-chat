//! Le réglage du modèle et de la réflexion en session : une recette par agent,
//! l'écran relu à chaque étape, aucun « appliqué » sans la confirmation de
//! l'agent — et, chez Claude, JAMAIS d'Entrée dans le sélecteur (Entrée
//! enregistre le choix comme défaut global ; `s` le garde à la session).
//!
//! Le faux port est un petit automate : chaque frappe fait évoluer l'écran
//! comme le ferait l'agent — les écrans viennent des textes relevés dans les
//! binaires (non observés en vrai, cf. `reglage.rs`).

use domain::ids::PaneId;
use domain::observation::{AgentListObservation, ScreenObservation, Snapshot, UpstreamPing};
use domain::ports::{
    ControlPort, CreateTab, CreatedTab, FocusEvidence, PortFuture, PromptEvidence, ScreenRead,
    StartAgent, StartEvidence, StopEvidence, TargetedKeys, TargetedPrompt, TargetedText,
    TransportEvidence,
};
use interactions::reglage::{
    DemandeReglage, EtatReglage, alias_claude, effort_affiche, ligne_surlignee, regle_modele,
};
use std::sync::Mutex;

/// Une frappe reçue par le faux port.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Frappe {
    Texte(String),
    Touches(Vec<String>),
}

type Reaction = Box<dyn Fn(&Frappe, &str) -> Option<String> + Send + Sync>;

struct FauxPort {
    ecran: Mutex<String>,
    frappes: Mutex<Vec<Frappe>>,
    reagit: Reaction,
}

impl FauxPort {
    fn nouveau(ecran: &str, reagit: Reaction) -> Self {
        Self {
            ecran: Mutex::new(ecran.to_string()),
            frappes: Mutex::new(Vec::new()),
            reagit,
        }
    }

    fn recoit(&self, frappe: Frappe) {
        let mut ecran = self
            .ecran
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(suivant) = (self.reagit)(&frappe, &ecran) {
            *ecran = suivant;
        }
        self.frappes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(frappe);
    }

    fn frappes(&self) -> Vec<Frappe> {
        self.frappes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

fn livre() -> TransportEvidence {
    TransportEvidence {
        outcome: domain::ports::TransportOutcome::Delivered,
        partial: Vec::new(),
        detail: String::new(),
    }
}

impl ControlPort for FauxPort {
    fn ping(&self) -> PortFuture<'_, UpstreamPing> {
        Box::pin(async {
            Ok(UpstreamPing {
                protocol: 1,
                version: String::from("0.1.18"),
            })
        })
    }
    fn snapshot(&self) -> PortFuture<'_, Snapshot> {
        Box::pin(async { Ok(Snapshot::default()) })
    }
    fn list_agents(&self) -> PortFuture<'_, AgentListObservation> {
        Box::pin(async { Ok(AgentListObservation { agents: Vec::new() }) })
    }
    fn read_screen(&self, request: ScreenRead) -> PortFuture<'_, ScreenObservation> {
        Box::pin(async move {
            let texte = self
                .ecran
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            Ok(ScreenObservation {
                pane_id: request.pane,
                text: Some(texte),
                truncated: Some(false),
                source: None,
            })
        })
    }
    fn prompt(&self, request: TargetedPrompt) -> PortFuture<'_, PromptEvidence> {
        Box::pin(async move {
            Ok(PromptEvidence::Confirmed {
                fingerprint: request.fingerprint,
            })
        })
    }
    fn send_text(&self, request: TargetedText) -> PortFuture<'_, TransportEvidence> {
        self.recoit(Frappe::Texte(request.text));
        Box::pin(async { Ok(livre()) })
    }
    fn send_keys(&self, request: TargetedKeys) -> PortFuture<'_, TransportEvidence> {
        self.recoit(Frappe::Touches(request.keys));
        Box::pin(async { Ok(livre()) })
    }
    fn focus(&self, _pane: PaneId) -> PortFuture<'_, FocusEvidence> {
        Box::pin(async { Ok(FocusEvidence::Focused) })
    }
    fn create_tab(&self, _request: CreateTab) -> PortFuture<'_, CreatedTab> {
        Box::pin(async {
            Ok(CreatedTab {
                tab: domain::ids::TabId::from(String::from("t")),
                pane: PaneId::from(String::from("p")),
            })
        })
    }
    fn start_agent(&self, _request: StartAgent) -> PortFuture<'_, StartEvidence> {
        Box::pin(async {
            Ok(StartEvidence::Started {
                pane: PaneId::from(String::from("p")),
            })
        })
    }
    fn stop(&self, _pane: PaneId) -> PortFuture<'_, StopEvidence> {
        Box::pin(async { Ok(StopEvidence::InterruptSent) })
    }
    fn fermer_onglet(&self, _tab: domain::ids::TabId) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn pane() -> PaneId {
    PaneId::from(String::from("t7"))
}

fn demande<'a>(
    kind: &'a str,
    modele: Option<&'a str>,
    reflexion: Option<&'a str>,
) -> DemandeReglage<'a> {
    DemandeReglage {
        kind,
        modele,
        reflexion,
    }
}

fn texte(t: &str) -> Frappe {
    Frappe::Texte(t.to_string())
}

fn touches(t: &[&str]) -> Frappe {
    Frappe::Touches(t.iter().map(|k| (*k).to_string()).collect())
}

fn regle(port: &FauxPort, d: DemandeReglage<'_>) -> interactions::reglage::ReglageModele {
    regle_modele(port, &pane(), d)
        .unwrap_or_else(|cause| panic!("un refus est une valeur : {cause}"))
}

// ---------------------------------------------------------------------------
// Garde commune
// ---------------------------------------------------------------------------

#[test]
fn une_valeur_de_plusieurs_mots_n_est_jamais_tapee() {
    // ⚠️ Collée puis validée par Entrée : « opus\n/clear » ferait passer une
    // seconde commande.
    let port = FauxPort::nouveau("", Box::new(|_, _| None));
    for valeur in ["opus\n/clear", "gpt-5 --reasoning=max", ""] {
        let issue = regle(&port, demande("codex", Some(valeur), None));
        assert_eq!(issue.etat, EtatReglage::Refuse, "{valeur:?} : {issue:?}");
    }
    assert!(port.frappes().is_empty());
}

#[test]
fn opencode_et_un_type_inconnu_sont_refuses_sans_rien_taper() {
    let port = FauxPort::nouveau("", Box::new(|_, _| None));
    for kind in ["opencode", "cursor", "zsh"] {
        let issue = regle(&port, demande(kind, Some("x"), None));
        assert_eq!(issue.etat, EtatReglage::Refuse, "{kind}");
        assert!(issue.detail.contains("nothing was sent"), "{issue:?}");
    }
    assert!(port.frappes().is_empty());
}

// ---------------------------------------------------------------------------
// Pi
// ---------------------------------------------------------------------------

fn pi_qui_confirme() -> Reaction {
    Box::new(|frappe, ecran| match frappe {
        Frappe::Texte(t) if t.starts_with("/model ") => {
            let id = t
                .trim_start_matches("/model ")
                .rsplit('/')
                .next()
                .unwrap_or("");
            Some(format!("{ecran}\nModel: {id}"))
        }
        Frappe::Texte(t) if t.starts_with("/thinking ") => Some(format!(
            "{ecran}\nThinking level: {}",
            t.trim_start_matches("/thinking ")
        )),
        _ => None,
    })
}

#[test]
fn pi_tape_ses_deux_commandes_et_lit_leurs_confirmations() {
    let port = FauxPort::nouveau("π", pi_qui_confirme());
    let issue = regle(
        &port,
        demande("pi", Some("anthropic/claude-opus-5"), Some("high")),
    );
    assert_eq!(issue.etat, EtatReglage::Applique, "{issue:?}");
    assert!(issue.detail.contains("Model: claude-opus-5"), "{issue:?}");
    assert!(issue.detail.contains("Thinking level: high"), "{issue:?}");
    assert_eq!(
        port.frappes(),
        [
            texte("/model anthropic/claude-opus-5"),
            texte("/thinking high")
        ]
    );
}

#[test]
fn pi_sans_confirmation_du_modele_n_envoie_pas_la_reflexion() {
    // Sans correspondance exacte, Pi ouvre son sélecteur : on s'arrête là.
    let port = FauxPort::nouveau("π", Box::new(|_, e| Some(format!("{e}\nSelect a model"))));
    let issue = regle(&port, demande("pi", Some("inconnu/x"), Some("high")));
    assert_eq!(issue.etat, EtatReglage::Transmis, "{issue:?}");
    assert_eq!(port.frappes(), [texte("/model inconnu/x")]);
}

#[test]
fn pi_qui_refuse_le_niveau_le_dit() {
    let port = FauxPort::nouveau(
        "π",
        Box::new(|_, e| {
            Some(format!(
                "{e}\nUnknown thinking level \"ultra\". Available levels: off, low"
            ))
        }),
    );
    let issue = regle(&port, demande("pi", None, Some("ultra")));
    assert_eq!(issue.etat, EtatReglage::Refuse, "{issue:?}");
    assert!(issue.detail.contains("Unknown thinking level"), "{issue:?}");
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

#[test]
fn codex_change_modele_et_raisonnement_d_une_seule_commande() {
    let port = FauxPort::nouveau(
        "›",
        Box::new(|_, e| Some(format!("{e}\n• Model changed to gpt-5.1-codex high"))),
    );
    let issue = regle(&port, demande("codex", Some("gpt-5.1-codex"), Some("high")));
    assert_eq!(issue.etat, EtatReglage::Applique, "{issue:?}");
    assert_eq!(
        port.frappes(),
        [texte("/model gpt-5.1-codex --reasoning=high")]
    );

    // La réflexion seule garde le modèle courant.
    let port = FauxPort::nouveau(
        "›",
        Box::new(|_, e| Some(format!("{e}\n• Model changed to gpt-5 low"))),
    );
    let _ = regle(&port, demande("codex", None, Some("low")));
    assert_eq!(port.frappes(), [texte("/model --reasoning=low")]);
}

#[test]
fn codex_qui_refuse_l_emporte_sur_une_ancienne_confirmation() {
    // Une confirmation PLUS ANCIENNE reste dans l'historique : c'est la
    // dernière ligne qui parle.
    let port = FauxPort::nouveau(
        "• Model changed to gpt-5 high",
        Box::new(|_, e| {
            Some(format!(
                "{e}\nReasoning level 'ultra' is unavailable for gpt-5. Available: low, medium, high"
            ))
        }),
    );
    let issue = regle(&port, demande("codex", Some("gpt-5"), Some("ultra")));
    assert_eq!(issue.etat, EtatReglage::Refuse, "{issue:?}");
    assert!(issue.detail.contains("unavailable"), "{issue:?}");
}

// ---------------------------------------------------------------------------
// devin
// ---------------------------------------------------------------------------

#[test]
fn devin_refuse_la_reflexion_avant_de_rien_taper() {
    let port = FauxPort::nouveau("", Box::new(|_, _| None));
    let issue = regle(&port, demande("devin", Some("opus"), Some("high")));
    assert_eq!(issue.etat, EtatReglage::Refuse, "{issue:?}");
    assert!(
        port.frappes().is_empty(),
        "un demi-succès n'est voulu par personne"
    );

    let port = FauxPort::nouveau(
        "",
        Box::new(|_, e| Some(format!("{e}\nModel set to Claude Opus"))),
    );
    let issue = regle(&port, demande("devin", Some("opus"), None));
    assert_eq!(issue.etat, EtatReglage::Applique, "{issue:?}");
    assert_eq!(port.frappes(), [texte("/model opus")]);
}

// ---------------------------------------------------------------------------
// Claude Code — le sélecteur
// ---------------------------------------------------------------------------

const EFFORTS: [&str; 5] = ["Low", "Medium", "High", "xHigh", "Max"];

/// Un sélecteur de Claude : `/model` l'ouvre, les lettres filtrent, `down`
/// surligne le premier modèle filtré, ←/→ règlent l'effort, `s` applique pour
/// la session. `confirmation` : la boîte « Switch model? » s'ouvre après `s`,
/// curseur sur l'option donnée.
fn claude(
    premier: &'static str,
    effort_initial: usize,
    confirmation: Option<&'static str>,
) -> Reaction {
    let etat = Mutex::new((String::new(), effort_initial, false));
    Box::new(move |frappe, ecran| {
        let mut etat = etat
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (filtre, effort, dans_liste) = &mut *etat;
        let selecteur = |filtre: &str, effort: usize, liste: bool| {
            let curseur = if liste { "❯ " } else { "  " };
            format!(
                "Select model\nType to filter: {filtre}\n{curseur}{premier}\n  Sonnet\n\n◉ {} effort ←/→ adjust",
                EFFORTS[effort]
            )
        };
        match frappe {
            Frappe::Texte(t) if t == "/model" => Some(selecteur("", *effort, false)),
            Frappe::Touches(k) if k == &["down"] => {
                *dans_liste = true;
                Some(selecteur(filtre, *effort, true))
            }
            Frappe::Touches(k) if k == &["right"] => {
                *effort = (*effort + 1).min(EFFORTS.len() - 1);
                Some(selecteur(filtre, *effort, *dans_liste))
            }
            Frappe::Touches(k) if k == &["left"] => {
                *effort = effort.saturating_sub(1);
                Some(selecteur(filtre, *effort, *dans_liste))
            }
            Frappe::Touches(k) if k == &["s"] && *dans_liste => Some(match confirmation {
                Some(option) => format!(
                    "Switch model?\nThis conversation is cached for the current model.\n{}\n{}",
                    if option == "yes" {
                        "❯ 1. Yes, switch to Opus"
                    } else {
                        "  1. Yes, switch to Opus"
                    },
                    if option == "yes" {
                        "  2. No, go back"
                    } else {
                        "❯ 2. No, go back"
                    },
                ),
                None => format!(
                    "> \n⎿ Set model to Opus for this session only with {} effort",
                    EFFORTS[*effort].to_lowercase()
                ),
            }),
            Frappe::Touches(k) if k == &["enter"] && ecran.contains("Switch model?") => {
                Some(format!(
                    "⎿ Set model to Opus for this session only with {} effort",
                    EFFORTS[*effort].to_lowercase()
                ))
            }
            Frappe::Touches(k) if k == &["escape"] => Some(String::from("> ")),
            Frappe::Touches(k) if !*dans_liste => {
                filtre.push_str(&k.concat());
                Some(selecteur(filtre, *effort, false))
            }
            _ => None,
        }
    })
}

#[test]
fn claude_passe_par_le_selecteur_et_la_touche_s_jamais_par_entree() {
    let port = FauxPort::nouveau("> ", claude("Opus", 2, None));
    let issue = regle(&port, demande("claude", Some("opus"), Some("max")));
    assert_eq!(issue.etat, EtatReglage::Applique, "{issue:?}");
    assert!(issue.detail.contains("for this session only"), "{issue:?}");
    assert_eq!(
        port.frappes(),
        [
            texte("/model"),
            touches(&["o", "p", "u", "s"]),
            touches(&["down"]),
            touches(&["right"]),
            touches(&["right"]),
            touches(&["s"]),
        ]
    );
    // ⚠️ Entrée enregistrerait le choix comme défaut de ~/.claude/settings.json.
    assert!(!port.frappes().contains(&touches(&["enter"])));
}

#[test]
fn claude_descend_l_effort_par_la_gauche() {
    let port = FauxPort::nouveau("> ", claude("Opus", 4, None));
    let issue = regle(
        &port,
        demande("claude", Some("claude-opus-5-5"), Some("medium")),
    );
    assert_eq!(issue.etat, EtatReglage::Applique, "{issue:?}");
    let gauches = port
        .frappes()
        .iter()
        .filter(|f| **f == touches(&["left"]))
        .count();
    assert_eq!(gauches, 3);
}

#[test]
fn claude_sur_le_mauvais_modele_referme_et_ne_valide_rien() {
    let port = FauxPort::nouveau("> ", claude("Sonnet", 2, None));
    let issue = regle(&port, demande("claude", Some("opus"), None));
    assert_eq!(issue.etat, EtatReglage::Refuse, "{issue:?}");
    let frappes = port.frappes();
    assert!(frappes.contains(&touches(&["escape"])), "{frappes:?}");
    assert!(!frappes.contains(&touches(&["s"])), "{frappes:?}");
}

#[test]
fn claude_confirme_le_changement_seulement_curseur_sur_oui() {
    let port = FauxPort::nouveau("> ", claude("Opus", 2, Some("yes")));
    let issue = regle(&port, demande("claude", Some("opus"), None));
    assert_eq!(issue.etat, EtatReglage::Applique, "{issue:?}");
    assert_eq!(port.frappes().last(), Some(&touches(&["enter"])));

    let port = FauxPort::nouveau("> ", claude("Opus", 2, Some("no")));
    let issue = regle(&port, demande("claude", Some("opus"), None));
    assert_eq!(issue.etat, EtatReglage::Refuse, "{issue:?}");
    assert!(!port.frappes().contains(&touches(&["enter"])));
}

#[test]
fn claude_sans_modele_ou_hors_alias_ne_tape_rien() {
    let port = FauxPort::nouveau("> ", claude("Opus", 2, None));
    for d in [
        demande("claude", None, Some("high")),
        demande("claude", Some("gpt-5"), None),
        demande("claude", Some("opus"), Some("ultra")),
    ] {
        let issue = regle(&port, d);
        assert_eq!(issue.etat, EtatReglage::Refuse, "{d:?} : {issue:?}");
    }
    assert!(port.frappes().is_empty());
}

#[test]
fn les_lectures_d_ecran_de_claude() {
    assert_eq!(alias_claude("opus"), Some("opus"));
    assert_eq!(alias_claude("claude-sonnet-4-5"), Some("sonnet"));
    assert_eq!(alias_claude("Default"), Some("default"));
    assert_eq!(alias_claude("gpt-5"), None);
    assert_eq!(
        effort_affiche("❯ Opus\n◉ xHigh effort (default) ←/→ adjust").as_deref(),
        Some("xhigh")
    );
    assert_eq!(effort_affiche("Effort not supported for Haiku"), None);
    assert_eq!(
        ligne_surlignee("  Default\n❯ Opus 5.5 · Most capable\n  Sonnet").as_deref(),
        Some("Opus 5.5 · Most capable")
    );
}
