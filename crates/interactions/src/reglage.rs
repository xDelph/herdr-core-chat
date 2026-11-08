//! Changer le **modèle** et la **réflexion** d'un agent en cours de session —
//! une recette par agent, et l'écran relu à chaque étape.
//!
//! ⚠️ **Non vérifié par exécution** (décision du 2026-11-08 : « code
//! seulement »). Chaque recette vient du code livré de l'agent (bundle, binaire,
//! documentation embarquée), pas d'une observation. D'où la discipline : rien ne
//! se dit « appliqué » sans le texte de confirmation de l'agent à l'écran ; un
//! écran inattendu arrête la recette (le sélecteur ouvert est refermé par
//! `escape`) et le dit.
//!
//! Les recettes, relevées le 2026-11-08 :
//!
//! | agent | modèle | réflexion | confirmation lue |
//! |---|---|---|---|
//! | Pi 1.5.0 | `/model <fournisseur/id>` | `/thinking <niveau>` | `Model: <id>`, `Thinking level: <niveau>` |
//! | Codex 0.166 | `/model <id> --reasoning=<niveau>` | idem (`/model --reasoning=…`) | `Model changed to` |
//! | devin 3000.11 | `/model <nom>` | **aucune commande** (`Alt+T` fait tourner) | `Model set to` |
//! | Claude Code 2.1.319 | sélecteur `/model`, touche `s` | ←/→ dans le sélecteur | `Set model to … for this session only` |
//!
//! ⚠️ **Claude passe par son sélecteur, jamais par `/model <alias>`** :
//! tapées, `/model` et `/effort` ENREGISTRENT le choix comme défaut de
//! `~/.claude/settings.json` (toutes les sessions suivantes du Mac). Seule la
//! touche `s` du sélecteur (« use this session only ») ne touche qu'à la
//! session — décision de l'utilisateur, 2026-11-08.
//!
//! ⚠️ **opencode n'est pas piloté** : ses sélecteurs (`/models`, `/variants`)
//! n'ont pas de commande à argument, ils montrent des NOMS (« GPT-5 ») là où
//! son catalogue donne des identifiants (`openai/gpt-5`), et la ligne choisie
//! n'y est marquée que par la COULEUR — `terminal.read` rend du texte seul.
//! On ne pourrait pas vérifier ce qu'`enter` validerait.

use crate::repondre::{attends, relit_apres_frappe};
use domain::errors::PortError;
use domain::ids::PaneId;
use domain::ports::{ControlPort, ScreenRead, TargetedKeys, TargetedText};

/// Ce qui s'est réellement passé.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EtatReglage {
    /// L'agent a affiché sa confirmation.
    Applique,
    /// Quelque chose a été tapé, mais aucune confirmation n'est lue : rien
    /// n'est prouvé, dans un sens ou dans l'autre.
    Transmis,
    /// Rien n'a changé — et la cause est nommée.
    Refuse,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReglageModele {
    pub etat: EtatReglage,
    pub detail: String,
}

/// La demande : un modèle, un niveau de réflexion, ou les deux.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DemandeReglage<'a> {
    /// Le type d'agent tel que le domaine le nomme (`claude`, `codex`, `pi`…).
    pub kind: &'a str,
    pub modele: Option<&'a str>,
    pub reflexion: Option<&'a str>,
}

/// Les types dont le changement de MODÈLE en session a une recette.
pub const KINDS_MODELE_EN_SESSION: [&str; 4] = ["pi", "codex", "devin", "claude"];
/// Les types dont le changement de RÉFLEXION en session a une recette.
pub const KINDS_REFLEXION_EN_SESSION: [&str; 3] = ["pi", "codex", "claude"];

pub type PortPourReglage = dyn ControlPort + 'static;

fn refuse(detail: impl Into<String>) -> ReglageModele {
    ReglageModele {
        etat: EtatReglage::Refuse,
        detail: detail.into(),
    }
}

fn nettoie(valeur: Option<&str>) -> Option<&str> {
    valeur.map(str::trim).filter(|v| !v.is_empty())
}

/// Un argument de commande : un seul mot, sans caractère de contrôle. Une
/// commande tapée est collée PUIS validée par Entrée — un saut de ligne ou une
/// espace y ferait passer une seconde commande.
fn mot_sur(valeur: &str) -> bool {
    !valeur.is_empty()
        && valeur.len() <= 200
        && valeur
            .chars()
            .all(|c| !c.is_whitespace() && !c.is_control())
}

/// Change le modèle et/ou la réflexion de l'agent du pane.
///
/// Ne lève que pour une panne du transport ; un refus est une **valeur**.
pub fn regle_modele(
    port: &PortPourReglage,
    pane: &PaneId,
    demande: DemandeReglage<'_>,
) -> Result<ReglageModele, PortError> {
    let modele = nettoie(demande.modele);
    let reflexion = nettoie(demande.reflexion);
    if modele.is_none() && reflexion.is_none() {
        return Ok(refuse(
            "nothing to change: choose a model or a thinking level",
        ));
    }
    if let Some(valeur) = modele.into_iter().chain(reflexion).find(|v| !mot_sur(v)) {
        return Ok(refuse(format!(
            "“{valeur}” is not a single word — nothing was sent"
        )));
    }
    match demande.kind {
        "pi" => regle_pi(port, pane, modele, reflexion),
        "codex" => regle_codex(port, pane, modele, reflexion),
        "devin" => regle_devin(port, pane, modele, reflexion),
        "claude" => regle_claude(port, pane, modele, reflexion),
        "opencode" => Ok(refuse(
            "opencode's model and variant pickers cannot be driven safely from here \
             (the selected row is only shown by colour) — change them on the Mac \
             (ctrl+x m, ctrl+t); nothing was sent",
        )),
        autre => Ok(refuse(format!(
            "no in-session model command is known for “{autre}” — nothing was sent"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Lecture de l'écran
// ---------------------------------------------------------------------------

fn lit(port: &PortPourReglage, pane: &PaneId) -> Result<String, PortError> {
    Ok(attends(&mut port.read_screen(ScreenRead {
        pane: pane.clone(),
        max_lines: 200,
    }))?
    .text
    .unwrap_or_default())
}

/// Relit jusqu'à ce que `cherche` réponde, borné (l'interface redessine en
/// quelques dizaines de millisecondes ; `terminal.send` a déjà attendu).
fn lit_jusqu_a(
    port: &PortPourReglage,
    pane: &PaneId,
    cherche: impl Fn(&str) -> bool,
) -> Result<String, PortError> {
    let mut ecran = lit(port, pane)?;
    for _ in 0..RELECTURES_CONFIRMATION {
        if cherche(&ecran) {
            break;
        }
        std::thread::sleep(INTERVALLE_CONFIRMATION);
        ecran = lit(port, pane)?;
    }
    Ok(ecran)
}

const RELECTURES_CONFIRMATION: usize = 6;
const INTERVALLE_CONFIRMATION: std::time::Duration = std::time::Duration::from_millis(150);

/// La DERNIÈRE ligne qui contient `motif` (casse ignorée) : la confirmation la
/// plus récente, pas une plus ancienne restée dans l'historique.
fn derniere_ligne_avec(ecran: &str, motif: &str) -> Option<String> {
    let motif = motif.to_lowercase();
    ecran
        .lines()
        .rev()
        .find(|ligne| ligne.to_lowercase().contains(&motif))
        .map(|ligne| ligne.trim().to_string())
}

fn contient(ecran: &str, motif: &str) -> bool {
    ecran.to_lowercase().contains(&motif.to_lowercase())
}

fn tape(port: &PortPourReglage, pane: &PaneId, commande: &str) -> Result<(), PortError> {
    attends(&mut port.send_text(TargetedText {
        pane: pane.clone(),
        text: commande.to_string(),
    }))?;
    Ok(())
}

fn touches(port: &PortPourReglage, pane: &PaneId, keys: &[&str]) -> Result<(), PortError> {
    attends(&mut port.send_keys(TargetedKeys {
        pane: pane.clone(),
        keys: keys.iter().map(|k| (*k).to_string()).collect(),
    }))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pi — `/model <id>`, `/thinking <niveau>`
// ---------------------------------------------------------------------------

/// Le nom nu d'un identifiant `fournisseur/modèle` : celui que Pi affiche.
fn nom_nu(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

fn regle_pi(
    port: &PortPourReglage,
    pane: &PaneId,
    modele: Option<&str>,
    reflexion: Option<&str>,
) -> Result<ReglageModele, PortError> {
    let mut faits = Vec::new();
    if let Some(id) = modele {
        tape(port, pane, &format!("/model {id}"))?;
        let attendu = format!("Model: {}", nom_nu(id));
        let ecran = lit_jusqu_a(port, pane, |e| contient(e, &attendu))?;
        if let Some(ligne) = derniere_ligne_avec(&ecran, &attendu) {
            faits.push(ligne);
        } else {
            // ⚠️ Sans correspondance exacte, Pi OUVRE son sélecteur filtré :
            // on ne choisit rien à sa place.
            return Ok(ReglageModele {
                etat: EtatReglage::Transmis,
                detail: format!(
                    "“/model {id}” was typed but Pi did not confirm it (no exact match opens its \
                     picker) — check the screen; the thinking level was not sent"
                ),
            });
        }
    }
    if let Some(niveau) = reflexion {
        tape(port, pane, &format!("/thinking {niveau}"))?;
        let attendu = format!("Thinking level: {niveau}");
        let ecran = lit_jusqu_a(port, pane, |e| {
            contient(e, &attendu)
                || contient(e, "Unknown thinking level")
                || contient(e, "does not support thinking")
        })?;
        if let Some(ligne) = derniere_ligne_avec(&ecran, "Unknown thinking level")
            .or_else(|| derniere_ligne_avec(&ecran, "does not support thinking"))
            .filter(|_| !contient(&ecran, &attendu))
        {
            return Ok(ReglageModele {
                etat: if faits.is_empty() {
                    EtatReglage::Refuse
                } else {
                    EtatReglage::Applique
                },
                detail: format!("{}Pi refused the thinking level: {ligne}", prefixe(&faits)),
            });
        }
        match derniere_ligne_avec(&ecran, &attendu) {
            Some(ligne) => faits.push(ligne),
            None => {
                return Ok(ReglageModele {
                    etat: EtatReglage::Transmis,
                    detail: format!(
                        "{}“/thinking {niveau}” was typed but Pi did not confirm it",
                        prefixe(&faits)
                    ),
                });
            }
        }
    }
    Ok(ReglageModele {
        etat: EtatReglage::Applique,
        detail: format!("{} (this session only)", faits.join(" · ")),
    })
}

fn prefixe(faits: &[String]) -> String {
    if faits.is_empty() {
        String::new()
    } else {
        format!("{} · ", faits.join(" · "))
    }
}

// ---------------------------------------------------------------------------
// Codex — `/model [id] [--reasoning=<niveau>]`
// ---------------------------------------------------------------------------

fn regle_codex(
    port: &PortPourReglage,
    pane: &PaneId,
    modele: Option<&str>,
    reflexion: Option<&str>,
) -> Result<ReglageModele, PortError> {
    let mut commande = String::from("/model");
    if let Some(id) = modele {
        commande.push(' ');
        commande.push_str(id);
    }
    if let Some(niveau) = reflexion {
        commande.push_str(" --reasoning=");
        commande.push_str(niveau);
    }
    tape(port, pane, &commande)?;
    let refus = ["is unavailable", "Unexpected argument", "Use /model to see"];
    let ecran = lit_jusqu_a(port, pane, |e| {
        contient(e, "Model changed to") || refus.iter().any(|r| contient(e, r))
    })?;
    // Le refus le plus récent l'emporte sur une confirmation plus ancienne.
    let confirme = derniere_position(&ecran, "Model changed to");
    let refuse_a = refus
        .iter()
        .filter_map(|r| derniere_position(&ecran, r).map(|p| (p, *r)))
        .max_by_key(|(p, _)| *p);
    match (confirme, refuse_a) {
        (_, Some((p, motif))) if confirme.is_none_or(|c| p > c) => Ok(refuse(format!(
            "Codex refused “{commande}”: {}",
            derniere_ligne_avec(&ecran, motif).unwrap_or_default()
        ))),
        (Some(_), _) => Ok(ReglageModele {
            etat: EtatReglage::Applique,
            detail: derniere_ligne_avec(&ecran, "Model changed to").unwrap_or_default(),
        }),
        _ => Ok(ReglageModele {
            etat: EtatReglage::Transmis,
            detail: format!("“{commande}” was typed but Codex did not confirm it"),
        }),
    }
}

/// Le rang (depuis le haut) de la dernière ligne qui contient `motif`.
fn derniere_position(ecran: &str, motif: &str) -> Option<usize> {
    let motif = motif.to_lowercase();
    ecran
        .lines()
        .enumerate()
        .filter(|(_, ligne)| ligne.to_lowercase().contains(&motif))
        .map(|(rang, _)| rang)
        .last()
}

// ---------------------------------------------------------------------------
// devin — `/model <nom>` ; la réflexion n'a pas de commande
// ---------------------------------------------------------------------------

fn regle_devin(
    port: &PortPourReglage,
    pane: &PaneId,
    modele: Option<&str>,
    reflexion: Option<&str>,
) -> Result<ReglageModele, PortError> {
    // ⚠️ Refusé AVANT de taper quoi que ce soit : un modèle changé sans le
    // niveau demandé serait un demi-succès que personne n'a voulu.
    if reflexion.is_some() {
        return Ok(refuse(
            "devin has no command to set a thinking level (Alt+T cycles it on the Mac) — \
             nothing was sent",
        ));
    }
    let Some(nom) = modele else {
        return Ok(refuse("nothing to change"));
    };
    let commande = format!("/model {nom}");
    tape(port, pane, &commande)?;
    let ecran = lit_jusqu_a(port, pane, |e| contient(e, "Model set to"))?;
    Ok(match derniere_ligne_avec(&ecran, "Model set to") {
        Some(ligne) => ReglageModele {
            etat: EtatReglage::Applique,
            detail: ligne,
        },
        None => ReglageModele {
            etat: EtatReglage::Transmis,
            detail: format!("“{commande}” was typed but devin did not confirm it"),
        },
    })
}

// ---------------------------------------------------------------------------
// Claude Code — le sélecteur `/model`, la touche `s`
// ---------------------------------------------------------------------------

/// Les alias que le sélecteur de Claude filtre (ceux de `claude --help`).
const ALIAS_CLAUDE: [&str; 4] = ["default", "opus", "sonnet", "haiku"];

/// L'alias à taper dans le filtre du sélecteur : l'alias lui-même, ou celui
/// que contient un identifiant complet (`claude-opus-4-…` → `opus`).
#[must_use]
pub fn alias_claude(modele: &str) -> Option<&'static str> {
    let bas = modele.to_lowercase();
    ALIAS_CLAUDE
        .iter()
        .find(|alias| bas == **alias)
        .or_else(|| ALIAS_CLAUDE[1..].iter().find(|alias| bas.contains(**alias)))
        .copied()
}

/// L'ordre des niveaux d'effort de Claude, du plus bas au plus haut.
const NIVEAUX_EFFORT_CLAUDE: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Le niveau d'effort affiché sous le modèle surligné (« High effort »,
/// « xHigh effort (default) ») ; `None` sans ligne lisible.
#[must_use]
pub fn effort_affiche(ecran: &str) -> Option<String> {
    ecran.lines().rev().find_map(|ligne| {
        let bas = ligne.to_lowercase();
        if bas.contains("effort not supported") {
            return None;
        }
        let avant = bas
            .split(" effort")
            .next()
            .filter(|_| bas.contains(" effort"))?;
        let mot = avant.split_whitespace().last()?;
        NIVEAUX_EFFORT_CLAUDE
            .contains(&mot)
            .then(|| mot.to_string())
    })
}

/// La ligne surlignée d'un sélecteur de Claude (curseur `❯`).
#[must_use]
pub fn ligne_surlignee(ecran: &str) -> Option<String> {
    ecran
        .lines()
        .map(str::trim_start)
        .rfind(|ligne| ligne.starts_with('❯'))
        .map(|ligne| ligne.trim_start_matches('❯').trim().to_string())
}

const TITRE_SELECTEUR_CLAUDE: &str = "Select model";
const PAS_EFFORT_MAXIMUM: usize = 8;

/// Referme le sélecteur de Claude s'il est encore ouvert (`escape` vide
/// d'abord le filtre, puis ferme) — borné, et seulement si son titre est là.
fn referme_selecteur(port: &PortPourReglage, pane: &PaneId) -> Result<(), PortError> {
    for _ in 0..3 {
        let ecran = lit(port, pane)?;
        if !contient(&ecran, TITRE_SELECTEUR_CLAUDE) {
            return Ok(());
        }
        touches(port, pane, &["escape"])?;
        relit_apres_frappe(port, pane, &ecran)?;
    }
    Ok(())
}

fn abandonne(
    port: &PortPourReglage,
    pane: &PaneId,
    detail: impl Into<String>,
) -> Result<ReglageModele, PortError> {
    referme_selecteur(port, pane)?;
    Ok(refuse(format!(
        "{} — the picker was closed, nothing changed",
        detail.into()
    )))
}

fn regle_claude(
    port: &PortPourReglage,
    pane: &PaneId,
    modele: Option<&str>,
    reflexion: Option<&str>,
) -> Result<ReglageModele, PortError> {
    // ⚠️ L'effort se règle DANS le sélecteur de modèle, sur le modèle
    // surligné : sans modèle, on ne saurait pas lequel surligner.
    let Some(modele) = modele else {
        return Ok(refuse(
            "Claude's effort is set in its model picker: choose the model too — nothing was sent",
        ));
    };
    let Some(alias) = alias_claude(modele) else {
        return Ok(refuse(format!(
            "“{modele}” is not one of Claude's picker aliases ({}) — nothing was sent",
            ALIAS_CLAUDE.join(", ")
        )));
    };
    if let Some(niveau) = reflexion
        && !NIVEAUX_EFFORT_CLAUDE.contains(&niveau)
    {
        return Ok(refuse(format!(
            "“{niveau}” is not a Claude effort level ({}) — nothing was sent",
            NIVEAUX_EFFORT_CLAUDE.join(", ")
        )));
    }

    // 1. Ouvrir le sélecteur.
    tape(port, pane, "/model")?;
    let ecran = lit_jusqu_a(port, pane, |e| contient(e, TITRE_SELECTEUR_CLAUDE))?;
    if !contient(&ecran, TITRE_SELECTEUR_CLAUDE) {
        return Ok(ReglageModele {
            etat: EtatReglage::Transmis,
            detail: String::from(
                "“/model” was typed but Claude's model picker did not appear — check the screen",
            ),
        });
    }

    // 2. Filtrer par l'alias — des TOUCHES : un collage serait suivi d'Entrée,
    //    qui enregistre le choix comme défaut global.
    let lettres: Vec<String> = alias.chars().map(String::from).collect();
    let lettres: Vec<&str> = lettres.iter().map(String::as_str).collect();
    touches(port, pane, &lettres)?;
    let ecran = relit_apres_frappe(port, pane, &ecran)?;
    if contient(&ecran, "No models match") {
        return abandonne(
            port,
            pane,
            format!("Claude's picker has no model matching “{alias}”"),
        );
    }

    // 3. Descendre dans la liste : le premier modèle filtré est surligné.
    touches(port, pane, &["down"])?;
    let mut ecran = relit_apres_frappe(port, pane, &ecran)?;
    let surlignee = ligne_surlignee(&ecran).unwrap_or_default();
    if !contient(&surlignee, alias) {
        return abandonne(
            port,
            pane,
            format!("the highlighted model is “{surlignee}”, not “{alias}”"),
        );
    }

    // 4. L'effort, un pas à la fois, relu à chaque pas.
    if let Some(niveau) = reflexion {
        if contient(&ecran, "Effort not supported") {
            return abandonne(
                port,
                pane,
                format!("Claude says “{surlignee}” has no effort setting"),
            );
        }
        let cible = rang_effort(niveau);
        let mut atteint = false;
        for _ in 0..PAS_EFFORT_MAXIMUM {
            let Some(affiche) = effort_affiche(&ecran) else {
                break;
            };
            let Some(rang) = rang_effort(&affiche) else {
                break;
            };
            if Some(rang) == cible {
                atteint = true;
                break;
            }
            let pas = if cible > Some(rang) { "right" } else { "left" };
            touches(port, pane, &[pas])?;
            let suivant = relit_apres_frappe(port, pane, &ecran)?;
            if effort_affiche(&suivant).as_deref() == Some(affiche.as_str()) {
                // Le pas n'a rien changé : bord de l'échelle, ou niveau non
                // offert par ce modèle.
                ecran = suivant;
                break;
            }
            ecran = suivant;
        }
        if !atteint {
            return abandonne(
                port,
                pane,
                format!(
                    "the effort could not be brought to “{niveau}” (shown: {})",
                    effort_affiche(&ecran).unwrap_or_else(|| String::from("nothing readable"))
                ),
            );
        }
    }

    // 5. `s` — « use this session only ».
    touches(port, pane, &["s"])?;
    let mut ecran = lit_jusqu_a(port, pane, |e| {
        contient(e, "Set model to")
            || contient(e, "Switch model?")
            || contient(e, "Change effort level?")
    })?;

    // 6. « Switch model? » : la conversation est en cache pour le modèle
    //    courant. Le changement est ce que l'utilisateur a demandé : on valide
    //    « Yes, switch » SEULEMENT si le curseur est dessus.
    if contient(&ecran, "Switch model?") || contient(&ecran, "Change effort level?") {
        let surlignee = ligne_surlignee(&ecran).unwrap_or_default();
        if !contient(&surlignee, "Yes, switch") {
            touches(port, pane, &["escape"])?;
            return Ok(refuse(format!(
                "Claude asked to confirm the switch and the cursor is on “{surlignee}”, not \
                 “Yes, switch” — cancelled, nothing changed"
            )));
        }
        touches(port, pane, &["enter"])?;
        ecran = lit_jusqu_a(port, pane, |e| contient(e, "Set model to"))?;
    }

    let Some(ligne) = derniere_ligne_avec(&ecran, "Set model to") else {
        return Ok(ReglageModele {
            etat: EtatReglage::Transmis,
            detail: String::from(
                "the choice was sent but Claude did not confirm it — check the screen",
            ),
        });
    };
    if contient(&ligne, "saved as your default") {
        return Ok(ReglageModele {
            etat: EtatReglage::Applique,
            detail: format!("{ligne} — ⚠️ Claude saved it as the default for new sessions"),
        });
    }
    Ok(ReglageModele {
        etat: EtatReglage::Applique,
        detail: ligne,
    })
}

fn rang_effort(niveau: &str) -> Option<usize> {
    let bas = niveau.to_lowercase();
    NIVEAUX_EFFORT_CLAUDE.iter().position(|n| *n == bas)
}
