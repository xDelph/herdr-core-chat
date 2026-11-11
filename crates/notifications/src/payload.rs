//! Les profils de payload métier (T045, E07) : qui, quoi, où — les trois
//! étiquettes distinctes (waiting/done/gone), le lien profond encodé, le
//! nom composé hors de l'écran.
//!
//! ⚠️ **`blocked` n'est pas `done`.** Les deux arrivent ici — l'événement
//! est « l'agent a arrêté de travailler » — mais l'un attend un humain et
//! l'autre non. Annoncer la même chose pour les deux rend la notification
//! inutile précisément quand elle compte le plus.
//!
//! ⚠️ **Un agent qui n'est plus là n'a pas « fini ».** Son processus s'est
//! terminé — mesuré : un vrai agent a rendu la main sans un mot, sans
//! rapport de plantage, et Claude Code avait supprimé son propre fichier
//! d'état, donc une sortie **propre**. Annoncer « has finished working »
//! ferait croire à une réussite, et c'est faux. La cause n'est pas
//! inventée : un pane qui disparaît ne dit rien, on dit qu'il n'est plus là.
//! Un terminal que herdr-core dit **fini** porte, lui, son code de sortie,
//! sa raison et son signal : ils sont dits tels quels ([`precise`]).
//!
//! ⚠️ **`url` mène à l'agent, pas à l'accueil**, et le fragment est
//! **encodé** : un `paneId` contient `:` (`wB:p16`), une adresse non
//! encodée serait relue de travers au retour.

use domain::naming::{PaneLabels, already_said, est_numerique, pane_label};
use domain::observation::AgentView;

/// Ce qu'il faut savoir d'un agent pour le **nommer** hors de l'écran —
/// sous-ensemble structurel d'`AgentView` : on ne demande que ce qui sert.
#[derive(Clone, Debug)]
pub struct IdentiteNotification {
    pub agent: Option<String>,
    pub name: Option<String>,
    pub pane_rank: Option<u64>,
    pub tab_label: Option<String>,
    pub tab_number: Option<u64>,
    pub tab_pane_count: u64,
    pub title: Option<String>,
    pub workspace_label: Option<String>,
}

impl IdentiteNotification {
    /// La vue d'un agent, réduite à ce que la notification demande.
    #[must_use]
    pub fn depuis_vue(vue: &AgentView) -> Self {
        Self {
            agent: vue.agent.as_ref().map(ToString::to_string),
            name: vue.name.as_ref().map(ToString::to_string),
            pane_rank: vue.pane_rank,
            tab_label: vue.tab_label.clone(),
            tab_number: vue.tab_number,
            tab_pane_count: vue.tab_pane_count,
            title: vue.title.clone(),
            workspace_label: vue.workspace_label.clone(),
        }
    }
}

/// Le statut de fin d'un agent — la cause du déclenchement (E06), réduite
/// aux trois profils de payload.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatutDeFin {
    /// `blocked` : l'agent attend une réponse — le plus urgent.
    EnAttente,
    /// `gone` : l'agent n'est plus là — pas « fini ».
    Disparu,
    /// `error` (herdr-core) : le programme s'est déclaré en erreur — pas
    /// « fini ».
    Erreur,
    /// Tout le reste : l'agent a fini de travailler.
    Fini,
}

/// Le payload d'une notification — le JSON exact envoyé au service push.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PayloadNotification {
    pub body: String,
    pub tag: String,
    pub title: String,
    pub url: String,
    /// Le genre, pour les réglages d'un appareil : [`GENRES`]. Hors du JSON
    /// envoyé — il ne sert qu'à choisir qui prévenir.
    pub genre: &'static str,
}

/// Les genres qu'un appareil peut couper dans ses réglages.
pub const GENRES: [&str; 4] = ["waiting", "done", "error", "gone"];

/// Le genre d'un statut de fin — le même mot que le début du `tag`.
#[must_use]
pub fn genre_de(statut: StatutDeFin) -> &'static str {
    match statut {
        StatutDeFin::EnAttente => "waiting",
        StatutDeFin::Disparu => "gone",
        StatutDeFin::Erreur => "error",
        StatutDeFin::Fini => "done",
    }
}

/// Le contenu d'une notification : trois étiquettes distinctes — une
/// question en attente ne doit pas être remplacée en silence par un
/// « finished » plus tardif pour le même pane, et une sortie ne doit pas
/// effacer une question restée sans réponse.
#[must_use]
pub fn payload_de_notification(
    titre: &str,
    pane_id: Option<&str>,
    statut: StatutDeFin,
) -> PayloadNotification {
    let connu = !titre.is_empty();
    let corps = |phrase: &str, generique: &str| {
        if connu {
            format!("{titre} {phrase}")
        } else {
            String::from(generique)
        }
    };
    let body = match statut {
        StatutDeFin::EnAttente => corps(
            "is waiting for your answer.",
            "An agent is waiting for your answer.",
        ),
        StatutDeFin::Disparu => corps("is no longer running.", "An agent is no longer running."),
        StatutDeFin::Erreur => corps("reported an error.", "An agent reported an error."),
        StatutDeFin::Fini => corps("has finished working.", "Your agent has finished working."),
    };
    let tag = match pane_id {
        None => String::from("herdr-agent"),
        Some(pane) => format!("{}-{pane}", genre_de(statut)),
    };
    let title = match statut {
        StatutDeFin::EnAttente => "Agent needs you",
        StatutDeFin::Disparu => "Agent stopped",
        StatutDeFin::Erreur => "Agent error",
        StatutDeFin::Fini => "Agent finished",
    };
    let url = match pane_id {
        None => String::from("/"),
        Some(pane) => format!("/#/agent/{}", encode_uri_component(pane)),
    };
    PayloadNotification {
        body,
        tag,
        title: String::from(title),
        url,
        genre: genre_de(statut),
    }
}

/// Ce que herdr-core dit de plus qu'un statut : la nature d'une attente
/// (`status.kind`) et la fin d'un terminal (code, raison, signal).
#[derive(Clone, Debug, Default)]
pub struct PrecisionsDeFin {
    pub attente: Option<domain::status::BlockedKind>,
    pub sortie: Option<domain::status::AgentExit>,
}

/// Le payload précisé : une attente dit CE QU'ELLE attend (« Permission
/// needed », « Sign-in needed »), une fin de terminal dit son code et sa
/// raison, expliqués (« was killed (code 1) »). Sans précision, le payload ne
/// change pas — rien n'est déduit.
#[must_use]
pub fn precise(
    mut payload: PayloadNotification,
    titre: &str,
    statut: StatutDeFin,
    precisions: &PrecisionsDeFin,
) -> PayloadNotification {
    use domain::status::BlockedKind;
    if statut == StatutDeFin::EnAttente
        && let Some(attente) = precisions.attente
    {
        payload.title = String::from(match attente {
            BlockedKind::Permission => "Permission needed",
            BlockedKind::Question => "Agent has a question",
            BlockedKind::Auth => "Sign-in needed",
        });
        if attente == BlockedKind::Auth {
            payload.body = if titre.is_empty() {
                String::from("An agent is waiting for you to sign in.")
            } else {
                format!("{titre} is waiting for you to sign in.")
            };
        }
    }
    if matches!(statut, StatutDeFin::Erreur | StatutDeFin::Fini)
        && let Some(sortie) = &precisions.sortie
    {
        let fin = sortie.explication();
        payload.title = String::from(if sortie.perdu() {
            "Agent lost"
        } else if sortie.en_echec() || sortie.reason.as_deref() == Some("killed") {
            "Agent exited with an error"
        } else {
            "Agent exited"
        });
        payload.body = if titre.is_empty() {
            format!("An agent {fin}.")
        } else {
            format!("{titre} {fin}.")
        };
    }
    payload
}

/// Comment nommer un agent dans une **notification**.
///
/// ⚠️ **Une notification doit dire QUI, et elle n'a aucun contexte.**
/// L'écran, lui, a le projet autour ; une notification arrive sur l'écran
/// verrouillé, parfois des heures plus tard, et un libellé qui suffit à
/// l'écran n'y suffit pas. Le nom se compose de ce qui existe, dans cet
/// ordre : le projet, l'onglet, le pane, le type d'agent. Ce qui manque est
/// omis, rien n'est inventé — et si tout manque, la chaîne est vide et le
/// payload reprend sa phrase générique.
#[must_use]
pub fn nom_de_notification(agent: Option<&IdentiteNotification>) -> String {
    let agent = match agent {
        Some(agent) => agent,
        None => return String::new(),
    };
    let projet =
        non_vide(agent.title.as_deref()).or_else(|| non_vide(agent.workspace_label.as_deref()));
    let mut parties = Vec::new();
    if let Some(projet) = projet {
        parties.push(projet.to_string());
    }
    if let Some(onglet) = nom_d_onglet(agent, projet) {
        parties.push(onglet);
    }
    // La place du pane vient de la MÊME fonction que l'écran (naming du
    // domaine) : la notification ne peut pas dire autre chose que la ligne
    // de la liste.
    if let Some(pane) = pane_label(&PaneLabels {
        name: agent.name.as_deref(),
        pane_rank: agent.pane_rank,
        tab_pane_count: agent.tab_pane_count,
        tab_label: agent.tab_label.as_deref(),
        workspace_label: agent.workspace_label.as_deref(),
    }) {
        parties.push(pane);
    }
    if let Some(kind) = non_vide(agent.agent.as_deref()) {
        parties.push(kind.to_string());
    }
    parties.join(" · ")
}

/// Le nom de l'onglet : son libellé quand il dit quelque chose, son numéro
/// sinon — mesuré : `tabLabel` est `null` sur 3 des 7 agents, `tabNumber`
/// renseigné sur les 7. Le libellé est écarté quand il répète le projet.
fn nom_d_onglet(agent: &IdentiteNotification, projet: Option<&str>) -> Option<String> {
    let label = non_vide(agent.tab_label.as_deref());
    if let Some(label) = label {
        let dit_par_le_projet = match projet {
            Some(projet) => already_said(projet, label),
            None => false,
        };
        if !est_numerique(label) && !dit_par_le_projet {
            return Some(label.to_string());
        }
    }
    if let Some(numero) = agent.tab_number {
        return Some(format!("tab {numero}"));
    }
    // Dernier recours : un libellé numérique reste plus exact que rien.
    label.map(str::to_string)
}

/// Non vide après nettoyage, ou rien.
fn non_vide(valeur: Option<&str>) -> Option<&str> {
    let propre = valeur?.trim();
    if propre.is_empty() {
        None
    } else {
        Some(propre)
    }
}

/// `encodeURIComponent` — la forme exacte du fil : tout sauf
/// `A-Za-z0-9-_.!~*'()` est encodé en `%XX`.
fn encode_uri_component(texte: &str) -> String {
    let mut sortie = String::new();
    for octet in texte.bytes() {
        match octet {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => sortie.push(octet as char),
            _ => sortie.push_str(&format!("%{octet:02X}")),
        }
    }
    sortie
}
