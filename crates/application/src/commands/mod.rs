//! Les cas d'usage de commande (T034+) : l'admission, le prompt, l'arrêt,
//! le focus.

use domain::errors::PortError;
use domain::ports::PortFuture;

/// Réexportés pour l'assembleur HTTP : `http-api` n'importe pas
/// `interactions` (graphe), et les deux gestes interactifs — répondre à une
/// question, trancher une confiance — y vivent.
pub use interactions::confiance::{EtatConfiance, ReponseConfiance};
pub use interactions::reglage::{
    DemandeReglage, EtatReglage, KINDS_MODELE_EN_SESSION, KINDS_REFLEXION_EN_SESSION, ReglageModele,
};
pub use interactions::repondre::{EtatReponse, ReponseQuestion};

pub mod admission;
pub mod focus;
pub mod launch;
pub mod prompt;
pub mod stop;

/// Répond à une question à choix — **admis par la file du pane** (T034) :
/// la lecture, la frappe et la relecture ne s'entrelacent jamais avec un
/// prompt, une confiance, un arrêt ou un focus sur le même pane. Même
/// signature que `interactions::repondre::repondre_question` : les handlers
/// n'ont rien à changer. La saturation est une erreur nommée (502 au fil),
/// rien n'a été envoyé.
pub fn repondre_question(
    port: &interactions::repondre::PortPourQuestion,
    pane: &domain::ids::PaneId,
    numero: i64,
    texte: Option<&str>,
    empreinte: Option<&str>,
) -> Result<ReponseQuestion, PortError> {
    admis(pane, || {
        interactions::repondre::repondre_question(port, pane, numero, texte, empreinte)
    })
}

/// Répond à l'invite de confiance — **admis par la file du pane** (T034),
/// même signature que `interactions::confiance::repondre_confiance`.
pub fn repondre_confiance(
    port: &dyn domain::ports::ControlPort,
    pane: &domain::ids::PaneId,
    accepte: bool,
    famille: Option<&str>,
) -> Result<ReponseConfiance, PortError> {
    admis(pane, || {
        interactions::confiance::repondre_confiance(port, pane, accepte, famille)
    })
}

/// Change le modèle et/ou la réflexion d'un agent en session — **admis par
/// la file du pane** : la recette (frappes, relectures) ne s'entrelace jamais
/// avec un prompt, une réponse ou un arrêt sur le même pane.
///
/// ⚠️ **Seulement sur un agent au repos, sans question à l'écran** (B1.3).
/// La recette tape `/model` puis Entrée : sur une question, Entrée validait un
/// choix que personne n'avait lu ; sur un agent au travail, la commande part
/// dans sa file et l'effet n'est jamais vérifié. `statut` vient d'une
/// lecture FRAÎCHE de Herdr (pas du cache), l'écran est relu sous l'admission.
pub fn regle_modele(
    port: &interactions::reglage::PortPourReglage,
    pane: &domain::ids::PaneId,
    statut: domain::status::AgentStatus,
    demande: DemandeReglage<'_>,
) -> Result<ReglageModele, PortError> {
    use domain::status::AgentStatus;
    let refus = |detail: String| ReglageModele {
        etat: interactions::reglage::EtatReglage::Refuse,
        detail,
    };
    if !matches!(statut, AgentStatus::Idle | AgentStatus::Done) {
        return Ok(refus(format!(
            "the agent is {} — change the model once it is idle; nothing was typed",
            statut
        )));
    }
    admis(pane, || {
        let mut lecture = port.read_screen(domain::ports::ScreenRead {
            pane: pane.clone(),
            max_lines: 200,
        });
        let ecran = pousse(&mut lecture)?.text.unwrap_or_default();
        if let Some(question) = interactions::ecran::question_de_l_ecran(&ecran) {
            return Ok(refus(format!(
                "the agent is asking “{}” — answer it first; nothing was typed",
                question.texte
            )));
        }
        if interactions::confiance::est_invite_confiance(&ecran) {
            return Ok(refus(String::from(
                "the agent is asking to trust its folder — answer it first; nothing was typed",
            )));
        }
        interactions::reglage::regle_modele(port, pane, demande)
    })
}

/// Une opération rendue en `PortError`, admise par la file du pane.
fn admis<T>(
    pane: &domain::ids::PaneId,
    effet: impl FnOnce() -> Result<T, PortError>,
) -> Result<T, PortError> {
    match admission::partage().admet(pane, effet) {
        Ok(valeur) => Ok(valeur),
        Err(Err(erreur)) => Err(erreur),
        // ⚠️ Un refus NOMMÉ (`pane_busy`), pas une panne de transport : rien
        // n'est parti, et le client peut réessayer (429 au fil).
        Err(Ok(admission::ErreurAdmission::Saturation)) => Err(PortError::Upstream {
            code: String::from("pane_busy"),
            message: admission::detail_saturation(pane),
        }),
    }
}

/// L'attente d'un port bloquant-déguisé, pour les handlers HTTP (lot 4 :
/// épingle, pièce jointe) — le même contrat que [`pousse`].
///
/// # Errors
/// L'erreur du port, telle quelle.
pub fn attend_port<T: Send>(fut: &mut PortFuture<'_, T>) -> Result<T, PortError> {
    pousse(fut)
}

/// L'attente d'un port bloquant-déguisé : le premier poll répond (tous les
/// faux et le transport réel T012 respectent ce contrat — `Pending` est un
/// branchement cassé, signalé comme tel, jamais une attente silencieuse).
pub(crate) fn pousse<T: Send>(fut: &mut PortFuture<'_, T>) -> Result<T, PortError> {
    use core::task::{Context, Poll, Waker};
    let mut contexte = Context::from_waker(Waker::noop());
    match fut.as_mut().poll(&mut contexte) {
        Poll::Ready(reponse) => reponse,
        Poll::Pending => {
            panic!("un port répond au premier poll ; Pending est un branchement cassé")
        }
    }
}
