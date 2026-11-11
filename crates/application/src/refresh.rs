//! `ServiceSnapshots` (T038) : le rafraîchissement partagé du snapshot et
//! du ping — TECH §4.
//!
//! - **Une seule requête amont à la fois** : les lectures simultanées sur
//!   une observation périmée se COALESCENT en un seul vol ; chacun rejoint
//!   le vol en cours (C23 — « one refresh for concurrent signals »).
//! - **Cache vide valide** : un snapshot vide est une observation comme une
//!   autre, servie dans sa fenêtre de fraîcheur.
//! - **Jamais de données périmées servies comme fraîches** : la panne du
//!   rafraîchissement est remontée telle quelle (502 au fil), l'ancienne
//!   observation n'est PAS présentée en remplacement.
//! - **Force** (admission, autorisation de racines de fichiers) : quitte la
//!   fenêtre de cache en partageant seulement un vol déjà en cours.
//! - **Santé** : ping coalescé séparément, TTL d'une seconde.
//! - **Battement** : `pulse()` est l'entrée du poller (2 s) —
//!   il ne frappe l'amont que quand l'observation vieillit.
//!
//! ⚠️ **L'horloge est toujours injectée** : `SourceMaintenant` — les
//! fenêtres (2 s, 1 s) se testent sans dormir, et le service ne lit jamais
//! `Instant::now()` directement.
//!
//! ⚠️ **Les verrous courts** : pendant l'appel amont (`snapshot`/`ping`)
//! AUCUN verrou n'est tenu (TECH §4 : ne pas tenir de verrou sur l'E/S) —
//! le vol est signalé par un drapeau, les points d'attente sont des
//! condvars.

use crate::commands::pousse;
use crate::state::ObservationBorne;
use domain::errors::PortError;
use domain::observation::UpstreamPing;
use domain::ports::ControlPort;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Le TTL de l'observation de santé : 1 s. La santé est coalescée
/// SÉPARÉMENT du snapshot (une panne de l'une n'entame pas l'autre).
pub const TTL_SANTE: Duration = Duration::from_secs(1);

/// L'attente maximale d'une lecture jointe à un vol en cours (B1.7).
///
/// ⚠️ **Jamais d'attente sans fin.** `vol.wait()` attendait le verdict du
/// porteur aussi longtemps qu'il le fallait : un porteur bloqué gelait
/// toutes les lectures du démon. Passé ce délai, la lecture jointe échoue
/// en `Timeout` — le vol, lui, continue.
pub const ATTENTE_VOL_MAX: Duration = Duration::from_secs(30);

/// L'état partagé d'un vol — ce qu'une garde rétablit si le porteur tombe.
trait EtatDeVol {
    fn abandonne(&mut self);
}

/// La garde du porteur d'un vol : si l'appel amont panique, elle libère le
/// vol, nomme la panne et réveille les joints (B1.7). Sans elle, `en_vol`
/// restait vrai pour toujours et chaque lecture suivante attendait un
/// verdict qui ne viendrait jamais.
struct GardeDeVol<'a, D: EtatDeVol> {
    depot: &'a Mutex<D>,
    reveil: &'a Condvar,
    armee: bool,
}

impl<D: EtatDeVol> Drop for GardeDeVol<'_, D> {
    fn drop(&mut self) {
        if self.armee {
            self.depot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .abandonne();
            self.reveil.notify_all();
        }
    }
}

fn panne_du_porteur() -> PortError {
    PortError::Transport {
        detail: String::from("the upstream request was abandoned (the reader panicked)"),
    }
}

/// La source d'un temps monotone — toujours injectée, jamais lue en dur.
pub trait SourceMaintenant: Send + Sync {
    /// L'instant monotone courant.
    fn maintenant(&self) -> Instant;
}

/// L'horloge réelle : `Instant::now()`.
#[derive(Default)]
pub struct MaintenantReelle;

impl SourceMaintenant for MaintenantReelle {
    fn maintenant(&self) -> Instant {
        Instant::now()
    }
}

/// Un ping horodaté par l'horloge injectée — la matière de `/api/health`.
#[derive(Clone, Debug)]
pub struct PingBorne {
    pub observe_a: Instant,
    pub ping: UpstreamPing,
}

/// La lecture d'un snapshot refusée — la cause amont est nommée, jamais
/// masquée par des données périmées.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ErreurLecture {
    /// Le rafraîchissement amont a échoué — l'ancienne observation n'est
    /// PAS servie comme fraîche : la lecture échoue avec la cause réelle.
    Amont { cause: PortError },
}

/// La lecture de santé refusée — TTL expiré et ping amont en panne.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ErreurSante {
    Amont { cause: PortError },
}

#[derive(Default)]
struct Depot {
    /// La DERNIÈRE observation committée (d'une génération, horodatée) —
    /// conservée même après un échec de rafraîchissement : les lectures
    /// refusent de la servir, sans l'effacer (continuité de génération).
    observation: Option<ObservationBorne>,
    /// Le nombre de vols réussis — croît de UNE génération par succès.
    generation: u64,
    /// Un rafraîchissement amont est en vol : quiconque arrive s'y joint.
    en_vol: bool,
    /// Le verdict du dernier vol — `Some` seulement en cas de panne.
    dernier_verdict: Option<PortError>,
    /// Les lectures qui attendent le verdict du vol en cours.
    joints: usize,
    /// Le numéro du dernier vol lancé — un joint attend la fin DE SON vol,
    /// jamais un réveil quelconque (B1.8).
    vol_numero: u64,
}

impl EtatDeVol for Depot {
    fn abandonne(&mut self) {
        self.en_vol = false;
        self.dernier_verdict = Some(panne_du_porteur());
    }
}

#[derive(Default)]
struct DepotSante {
    ping: Option<PingBorne>,
    en_vol: bool,
    dernier_verdict: Option<PortError>,
    vol_numero: u64,
}

impl EtatDeVol for DepotSante {
    fn abandonne(&mut self) {
        self.en_vol = false;
        self.dernier_verdict = Some(panne_du_porteur());
    }
}

/// Le service qui possède le port et distille une vue partagée de l'état
/// amont — partageable par `Arc` entre les routes (T056).
pub struct ServiceSnapshots {
    port: Arc<dyn ControlPort>,
    depot: Mutex<Depot>,
    vol: Condvar,
    sante: Mutex<DepotSante>,
    vol_sante: Condvar,
    maintenant: Arc<dyn SourceMaintenant>,
}

impl ServiceSnapshots {
    /// Crée le service avec une horloge **explicitement injectée** — jamais
    /// de dépendance cachée au temps réel (les fenêtres se testent sans
    /// dormir).
    #[must_use]
    pub fn nouveau(port: Arc<dyn ControlPort>, maintenant: Arc<dyn SourceMaintenant>) -> Self {
        Self {
            port,
            depot: Mutex::new(Depot::default()),
            vol: Condvar::new(),
            sante: Mutex::new(DepotSante::default()),
            vol_sante: Condvar::new(),
            maintenant,
        }
    }

    /// La lecture ordinaire : une observation fraîche (≤ 2 s, même vide)
    /// est servie sans toucher l'amont ; sinon la lecture REJOINT un
    /// rafraîchissement — une seule requête amont pour tous les signaux
    /// simultanés (C23) — et reçoit le verdict partagé (succès ou panne).
    pub fn lecture(&self) -> Result<ObservationBorne, ErreurLecture> {
        self.rafraichit(false)
    }

    /// La lecture forcée : l'admission du contrôle et l'autorisation des
    /// racines de fichiers exigent une vraie fraîcheur — on sort de la
    /// fenêtre de cache, en partageant seulement un vol déjà en cours.
    pub fn lecture_forcee(&self) -> Result<ObservationBorne, ErreurLecture> {
        self.rafraichit(true)
    }

    /// La santé : un ping coalescé au TTL d'une seconde.
    pub fn sante(&self) -> Result<PingBorne, ErreurSante> {
        loop {
            let mut depot = self.sante.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(borne) = &depot.ping
                && self.fraiche(borne.observe_a, TTL_SANTE)
            {
                return Ok(borne.clone());
            }
            if depot.en_vol {
                let numero = depot.vol_numero;
                let (attendu, attente) = self
                    .vol_sante
                    .wait_timeout_while(depot, ATTENTE_VOL_MAX, |depot| {
                        depot.en_vol && depot.vol_numero == numero
                    })
                    .unwrap_or_else(PoisonError::into_inner);
                depot = attendu;
                if attente.timed_out() {
                    return Err(ErreurSante::Amont {
                        cause: PortError::Timeout,
                    });
                }
                // Un vol plus récent a déjà commencé : on s'y joint.
                if depot.en_vol {
                    continue;
                }
                if let Some(cause) = &depot.dernier_verdict {
                    return Err(ErreurSante::Amont {
                        cause: cause.clone(),
                    });
                }
                continue;
            }
            depot.en_vol = true;
            depot.vol_numero += 1;
            depot.dernier_verdict = None;
            drop(depot);
            let mut garde = GardeDeVol {
                depot: &self.sante,
                reveil: &self.vol_sante,
                armee: true,
            };
            let mut requete = self.port.ping();
            let resultat = pousse(&mut requete);
            garde.armee = false;
            let mut depot = self.sante.lock().unwrap_or_else(PoisonError::into_inner);
            depot.en_vol = false;
            match resultat {
                Ok(ping) => {
                    depot.ping = Some(PingBorne {
                        observe_a: self.maintenant.maintenant(),
                        ping,
                    });
                    depot.dernier_verdict = None;
                    self.vol_sante.notify_all();
                    let ping = depot.ping.clone().unwrap_or_else(|| {
                        unreachable!("le ping vient d'être committé sous ce verrou")
                    });
                    return Ok(ping);
                }
                Err(cause) => {
                    depot.dernier_verdict = Some(cause.clone());
                    self.vol_sante.notify_all();
                    return Err(ErreurSante::Amont { cause });
                }
            }
        }
    }

    /// Le battement du poller (2 s) : ne frappe l'amont que quand
    /// l'observation vieillit — le retour porte l'échec pour le journal du
    /// poller, qui n'est PAS un point de décision.
    pub fn pulse(&self) -> Result<(), ErreurLecture> {
        self.rafraichit(false).map(drop)
    }

    /// La génération courante — l'outil des lectures « une même génération »
    /// (T039) et la mesure des changements committés.
    #[must_use]
    pub fn generation_actuelle(&self) -> u64 {
        self.depot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .generation
    }

    /// Le nombre de lectures jointes au vol en cours — l'observation qui
    /// permet de prouver le partage d'un vol sans dormir en espérant que
    /// les joints soient arrivés.
    #[must_use]
    pub fn joints_en_attente(&self) -> usize {
        self.depot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .joints
    }

    fn fraiche(&self, observe_a: Instant, fenetre: Duration) -> bool {
        self.maintenant
            .maintenant()
            .saturating_duration_since(observe_a)
            <= fenetre
    }

    fn rafraichit(&self, force: bool) -> Result<ObservationBorne, ErreurLecture> {
        loop {
            let mut depot = self.depot.lock().unwrap_or_else(PoisonError::into_inner);
            // Une observation fraîche est une observation valide : servie,
            // sans toucher l'amont — sauf en mode forcé.
            if !force
                && let Some(borne) = &depot.observation
                && borne.fraiche(self.maintenant.maintenant())
            {
                return Ok(borne.clone());
            }
            // Un vol est en cours : on s'y JOINT (une seule requête amont
            // pour les signaux simultanés) et on réévalue après son verdict.
            if depot.en_vol {
                let numero = depot.vol_numero;
                depot.joints += 1;
                let (attendu, attente) = self
                    .vol
                    .wait_timeout_while(depot, ATTENTE_VOL_MAX, |depot| {
                        depot.en_vol && depot.vol_numero == numero
                    })
                    .unwrap_or_else(PoisonError::into_inner);
                depot = attendu;
                depot.joints -= 1;
                if attente.timed_out() {
                    return Err(ErreurLecture::Amont {
                        cause: PortError::Timeout,
                    });
                }
                // ⚠️ Réveillé alors qu'un vol PLUS RÉCENT a déjà commencé
                // (B1.8) : son `dernier_verdict` vient d'être remis à zéro,
                // et servir l'observation maintenant serait servir celle
                // d'avant la panne. On rejoint le nouveau vol.
                if depot.en_vol {
                    continue;
                }
                match &depot.dernier_verdict {
                    // Le verdict partagé : une panne nomme la cause à tous
                    // les joints (jamais des données périmées en
                    // remplacement) — sauf en mode forcé, où le demandeur
                    // EXIGE une fraîcheur réelle et relance un vol.
                    Some(cause) if !force => {
                        return Err(ErreurLecture::Amont {
                            cause: cause.clone(),
                        });
                    }
                    Some(_) => continue,
                    // Succès : la génération commitée est fraîche pour
                    // TOUT le monde (force comprise) — servie telle quelle.
                    None => {
                        if let Some(borne) = &depot.observation {
                            return Ok(borne.clone());
                        }
                    }
                }
                continue;
            }
            // Porteur : la seule requête amont de cette génération.
            depot.en_vol = true;
            depot.vol_numero += 1;
            depot.dernier_verdict = None;
            drop(depot);
            let mut garde = GardeDeVol {
                depot: &self.depot,
                reveil: &self.vol,
                armee: true,
            };
            let mut photo = self.port.snapshot();
            let resultat = pousse(&mut photo);
            garde.armee = false;
            let mut depot = self.depot.lock().unwrap_or_else(PoisonError::into_inner);
            depot.en_vol = false;
            match resultat {
                Ok(snapshot) => {
                    depot.generation += 1;
                    let borne = ObservationBorne {
                        generation: depot.generation,
                        observe_a: self.maintenant.maintenant(),
                        snapshot,
                    };
                    depot.observation = Some(borne.clone());
                    depot.dernier_verdict = None;
                    self.vol.notify_all();
                    return Ok(borne);
                }
                Err(cause) => {
                    depot.dernier_verdict = Some(cause.clone());
                    self.vol.notify_all();
                    return Err(ErreurLecture::Amont { cause });
                }
            }
        }
    }
}
