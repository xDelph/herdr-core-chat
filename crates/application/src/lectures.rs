//! Les **lectures partagées** (H22) : « cette conversation a été lue jusqu'à
//! tel instant », tenu par le démon pour tous les appareils.
//!
//! ⚠️ **Retour du 2026-10-29.** Le « lu » vivait dans chaque navigateur : une
//! conversation lue sur le téléphone gardait son point « non lu » sur
//! l'ordinateur. Le démon est le seul endroit que tous les appareils voient.
//!
//! - Une lecture **ne recule jamais** : noter un instant plus ancien que le
//!   connu rend le connu.
//! - Borné : au-delà de [`MAXIMUM_LECTURES`] panes, les plus anciennes
//!   lectures sont oubliées (un pane oublié retombe sur la règle sans lecture).
//! - La persistance est un port : une panne d'écriture est **rendue** à
//!   l'appelant (qui la journalise), la lecture reste notée en mémoire.
//! - Ce n'est pas du contenu : un instant par pane. Lire ne vole jamais le
//!   focus du Mac (geste explicite).
//! - ⚠️ **Le « vu » est partagé avec Herdr.app** ([`partage_vu`], retour du
//!   2026-11-08) : une conversation lue ici éteint aussi le `done` de
//!   Herdr.app (label `herdr.seen`, que ses fenêtres et son app téléphone
//!   partagent déjà) — et l'adaptateur relit ce label dans l'autre sens.

use domain::errors::PortError;
use domain::ids::PaneId;
use domain::ports::ControlPort;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// Le nombre de panes retenus.
pub const MAXIMUM_LECTURES: usize = 512;

/// Où les lectures survivent au redémarrage du démon.
pub trait MagasinLectures: Send + Sync {
    /// Les lectures retenues ; une source absente est une table vide.
    ///
    /// # Errors
    /// La cause réelle d'une source illisible.
    fn charge(&self) -> Result<HashMap<String, u64>, String>;

    /// Écrit la table complète.
    ///
    /// # Errors
    /// La cause réelle d'une écriture refusée.
    fn ecrit(&self, lectures: &HashMap<String, u64>) -> Result<(), String>;
}

/// L'issue d'une lecture notée.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LectureNotee {
    /// L'instant retenu — jamais plus ancien que le précédent.
    pub lue_a: u64,
    /// La panne de persistance, s'il y en a eu une (la lecture vaut quand
    /// même jusqu'au redémarrage).
    pub panne: Option<String>,
}

/// Le service des lectures partagées.
pub struct ServiceLectures {
    lectures: Mutex<HashMap<String, u64>>,
    magasin: Option<Arc<dyn MagasinLectures>>,
}

impl ServiceLectures {
    /// Un service sans persistance (tests, démon sans dossier d'état).
    #[must_use]
    pub fn en_memoire() -> Self {
        Self {
            lectures: Mutex::new(HashMap::new()),
            magasin: None,
        }
    }

    /// Un service persistant, chargé depuis son magasin. Une source
    /// illisible n'empêche pas de démarrer : la cause est rendue, la table
    /// part vide.
    #[must_use]
    pub fn charge(magasin: Arc<dyn MagasinLectures>) -> (Self, Option<String>) {
        let (lectures, panne) = match magasin.charge() {
            Ok(lectures) => (borne(lectures), None),
            Err(cause) => (HashMap::new(), Some(cause)),
        };
        (
            Self {
                lectures: Mutex::new(lectures),
                magasin: Some(magasin),
            },
            panne,
        )
    }

    /// L'instant jusqu'où le pane a été lu, s'il l'a été.
    #[must_use]
    pub fn lue_a(&self, pane: &str) -> Option<u64> {
        self.lectures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(pane)
            .copied()
    }

    /// Note une lecture — l'instant retenu ne recule jamais.
    pub fn note(&self, pane: &str, instant: u64) -> LectureNotee {
        let mut lectures = self.lectures.lock().unwrap_or_else(PoisonError::into_inner);
        let connue = lectures.get(pane).copied();
        if connue.is_some_and(|connue| connue >= instant) {
            return LectureNotee {
                lue_a: connue.unwrap_or(instant),
                panne: None,
            };
        }
        lectures.insert(pane.to_string(), instant);
        if lectures.len() > MAXIMUM_LECTURES {
            *lectures = borne(std::mem::take(&mut *lectures));
        }
        let panne = self
            .magasin
            .as_ref()
            .and_then(|magasin| magasin.ecrit(&lectures).err());
        LectureNotee {
            lue_a: instant,
            panne,
        }
    }
}

/// Garde les [`MAXIMUM_LECTURES`] lectures les plus récentes.
fn borne(lectures: HashMap<String, u64>) -> HashMap<String, u64> {
    if lectures.len() <= MAXIMUM_LECTURES {
        return lectures;
    }
    let mut triees: Vec<(String, u64)> = lectures.into_iter().collect();
    triees.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    triees.truncate(MAXIMUM_LECTURES);
    triees.into_iter().collect()
}

/// Dit à l'amont que la conversation de `pane` a été vue jusqu'à `vu_a` :
/// `Some(tour)` quand un tour `done` vient d'être noté vu, `None` quand il
/// n'y avait rien à noter. Le port décide de ce qui est prouvé (un tour fini
/// après la lecture n'est jamais marqué).
///
/// # Errors
/// La cause réelle d'un amont qui refuse ou ne répond pas.
pub fn partage_vu(port: &dyn ControlPort, pane: &str, vu_a: u64) -> Result<Option<u64>, PortError> {
    let mut demande = port.marque_vu(PaneId::from(pane.to_string()), vu_a);
    crate::commands::pousse(&mut demande)
}
