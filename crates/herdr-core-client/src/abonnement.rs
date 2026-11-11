//! L'abonnement aux événements de herdr-core (`events.subscribe`, lot 4).
//!
//! ⚠️ **Il ne remplace pas la lecture : il la déclenche.** La veille relisait
//! `terminal.list` + `doc.get` toutes les 2 s : un « done » ou un « needs
//! you » attendait jusqu'à 2 s, et un tour plus court passait entre deux
//! lectures. Le flux réveille la veille dès qu'un fait change ; la lecture
//! complète reste la seule source de vérité (rien n'est reconstruit à partir
//! des événements), et la cadence de 2 s reste le filet si le flux se tait.
//!
//! Mesuré sur un core isolé 0.1.19 (2026-11-10) :
//! - la réponse de `events.subscribe` porte l'instantané (`seq`,
//!   `terminals`), puis chaque fait arrive en `{"method":"event","params":
//!   {"type", "terminal", "seq", "data"}}` ;
//! - `status` porte le statut ET les records (`data.status`, `data.records`)
//!   à chaque message du programme — une rafale pendant le travail ;
//! - `turn` (`{state, turn}`), `finished` (`{exit_code, reason, signal}`),
//!   `labels`, `created`, `title`… ;
//! - `{"method":"heartbeat","params":{"seq"}}` toutes les 15 s.
//!
//! ⚠️ **Seuls les changements qui comptent réveillent** ([`Pertinence`]) :
//! le mot d'état, la nature d'une attente, les records (id, état, titre),
//! un tour, une fin, un label, un titre dont le texte (hors spinner) change.
//! Le message d'activité seul (`msg`) et la taille, non : ils arrivent en
//! rafale et ne changent ni la liste ni les notifications.

use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Sans rien reçu pendant ce délai (deux battements et demi), le flux est
/// tenu pour mort : on se réabonne.
pub const SILENCE_MAX: Duration = Duration::from_secs(40);
/// L'attente avant un nouvel essai, doublée à chaque échec jusqu'au plafond.
pub const REESSAI_MIN: Duration = Duration::from_secs(1);
pub const REESSAI_MAX: Duration = Duration::from_secs(30);

/// Ce que l'abonnement dit à la veille.
pub trait RecepteurEvenements: Send + Sync {
    /// Le flux est branché (`true`, instantané reçu) ou perdu (`false`).
    fn branche(&self, branche: bool);
    /// Un signe de vie du flux (événement ou battement).
    fn signe_de_vie(&self);
    /// Un fait qui compte a changé : relire maintenant.
    fn reveille(&self);
}

/// Ce que l'on retient d'un terminal pour juger un événement `status` ou
/// `title` : seulement ce qui change la liste.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct Retenu {
    statut: String,
    titre: String,
}

/// Le juge des événements — pur, testé sans socket.
#[derive(Default, Debug)]
pub struct Pertinence {
    terminaux: HashMap<String, Retenu>,
}

/// Le titre utile : sans le spinner ni les glyphes de tête (Claude fait
/// tourner `✳ ⠂ ⠐ …` devant son titre pendant le travail).
fn titre_utile(titre: &str) -> String {
    titre
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim()
        .to_string()
}

/// L'empreinte d'un statut : le mot, la nature d'attente, et chaque record
/// (id, mot, titre) — jamais le `msg`, qui change à chaque outil.
fn empreinte_statut(data: &Value) -> String {
    let mot = |statut: Option<&Value>| {
        let statut = statut.filter(|s| !s.is_null());
        format!(
            "{}|{}",
            statut
                .and_then(|s| s.get("state"))
                .and_then(Value::as_str)
                .unwrap_or("-"),
            statut
                .and_then(|s| s.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("-"),
        )
    };
    let mut records: Vec<String> = data
        .get("records")
        .and_then(Value::as_object)
        .map(|records| {
            records
                .iter()
                .map(|(id, record)| {
                    format!(
                        "{id}={}:{}",
                        mot(Some(record)),
                        record.get("title").and_then(Value::as_str).unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    records.sort();
    format!("{}#{}", mot(data.get("status")), records.join(","))
}

impl Pertinence {
    /// Reprend l'instantané d'un abonnement (ou d'un `missed`).
    pub fn instantane(&mut self, terminaux: &[Value]) {
        self.terminaux.clear();
        for terminal in terminaux {
            let Some(adresse) = terminal.get("address").and_then(Value::as_str) else {
                continue;
            };
            self.terminaux.insert(
                adresse.to_string(),
                Retenu {
                    statut: empreinte_statut(terminal),
                    titre: titre_utile(terminal.get("title").and_then(Value::as_str).unwrap_or("")),
                },
            );
        }
    }

    /// Une notification du flux : faut-il relire ?
    pub fn juge(&mut self, notification: &Value) -> bool {
        let methode = notification.get("method").and_then(Value::as_str);
        if methode == Some("heartbeat") {
            return false;
        }
        let params = notification.get("params").unwrap_or(&Value::Null);
        if methode != Some("event") {
            // `missed` et tout ce qu'on ne connaît pas : un nouvel instantané
            // quand il y en a un, et une relecture dans tous les cas.
            if let Some(terminaux) = params.get("terminals").and_then(Value::as_array) {
                self.instantane(terminaux);
            }
            return true;
        }
        let genre = params.get("type").and_then(Value::as_str).unwrap_or("");
        let adresse = params
            .get("terminal")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let data = params.get("data").unwrap_or(&Value::Null);
        match genre {
            "status" => {
                let empreinte = empreinte_statut(data);
                let retenu = self.terminaux.entry(adresse).or_default();
                let change = retenu.statut != empreinte;
                retenu.statut = empreinte;
                change
            }
            "title" => {
                let titre = titre_utile(data.get("title").and_then(Value::as_str).unwrap_or(""));
                let retenu = self.terminaux.entry(adresse).or_default();
                let change = retenu.titre != titre;
                retenu.titre = titre;
                change
            }
            "size" => false,
            "removed" => {
                self.terminaux.remove(&adresse);
                true
            }
            // turn, finished, created, lost, restarted, keep, labels, doc, et
            // tout genre nouveau : relire.
            _ => true,
        }
    }
}

/// L'abonnement qui tourne : son fil et de quoi l'arrêter.
pub struct Abonnement {
    arret: Arc<AtomicBool>,
    flux: Arc<Mutex<Option<UnixStream>>>,
}

impl Abonnement {
    /// Démarre l'abonnement sur son propre fil : connexion, `hello`,
    /// `events.subscribe`, puis lecture du flux ; à la perte, nouvel essai
    /// après 1 s, 2 s… 30 s au plus. Un échec de démarrage du fil est rendu.
    ///
    /// # Errors
    /// Le fil n'a pas pu être créé.
    pub fn demarre(
        socket: PathBuf,
        recepteur: Arc<dyn RecepteurEvenements>,
    ) -> std::io::Result<Self> {
        let arret = Arc::new(AtomicBool::new(false));
        let flux: Arc<Mutex<Option<UnixStream>>> = Arc::new(Mutex::new(None));
        let (arret_fil, flux_fil) = (Arc::clone(&arret), Arc::clone(&flux));
        std::thread::Builder::new()
            .name(String::from("evenements-herdr"))
            .spawn(move || boucle(&socket, recepteur.as_ref(), &arret_fil, &flux_fil))?;
        Ok(Self { arret, flux })
    }

    /// Arrête l'abonnement : la connexion est fermée, le fil sort.
    pub fn arrete(&self) {
        self.arret.store(true, Ordering::SeqCst);
        if let Some(flux) = self
            .flux
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = flux.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for Abonnement {
    fn drop(&mut self) {
        self.arrete();
    }
}

fn boucle(
    socket: &std::path::Path,
    recepteur: &dyn RecepteurEvenements,
    arret: &AtomicBool,
    flux: &Mutex<Option<UnixStream>>,
) {
    let mut attente = REESSAI_MIN;
    let mut dernier_echec: Option<String> = None;
    while !arret.load(Ordering::SeqCst) {
        let issue = une_session(socket, recepteur, arret, flux);
        recepteur.branche(false);
        if arret.load(Ordering::SeqCst) {
            break;
        }
        match issue {
            // Une session qui a vécu remet l'attente au minimum.
            Ok(()) => {
                tracing::info!("événements herdr-core : flux fermé, réabonnement");
                attente = REESSAI_MIN;
                dernier_echec = None;
            }
            Err(cause) => {
                if dernier_echec.as_deref() != Some(cause.as_str()) {
                    tracing::warn!("événements herdr-core : {cause} — la veille reste à 2 s");
                    dernier_echec = Some(cause);
                }
            }
        }
        std::thread::sleep(attente);
        attente = (attente * 2).min(REESSAI_MAX);
    }
}

/// Une session d'abonnement, jusqu'à sa perte. `Ok` : elle a été branchée
/// puis s'est fermée ; `Err` : elle n'a jamais été branchée.
fn une_session(
    socket: &std::path::Path,
    recepteur: &dyn RecepteurEvenements,
    arret: &AtomicBool,
    flux: &Mutex<Option<UnixStream>>,
) -> Result<(), String> {
    let mut connexion = UnixStream::connect(socket).map_err(|e| format!("socket: {e}"))?;
    connexion
        .set_read_timeout(Some(SILENCE_MAX))
        .map_err(|e| format!("socket: {e}"))?;
    *flux.lock().unwrap_or_else(PoisonError::into_inner) =
        Some(connexion.try_clone().map_err(|e| format!("socket: {e}"))?);
    let mut lecteur = BufReader::new(connexion.try_clone().map_err(|e| format!("socket: {e}"))?);
    let mut ecrit = |id: u64, methode: &str, params: Value| {
        let mut ligne = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": methode, "params": params,
        })
        .to_string();
        ligne.push('\n');
        connexion
            .write_all(ligne.as_bytes())
            .map_err(|e| format!("write: {e}"))
    };
    let lis = |lecteur: &mut BufReader<UnixStream>| -> Result<Option<Value>, String> {
        let mut ligne = String::new();
        match lecteur.read_line(&mut ligne) {
            Ok(0) => Ok(None),
            Ok(_) if ligne.len() > crate::transport::CADRE_MAX_OCTETS => {
                Err(String::from("frame beyond 16 MiB"))
            }
            Ok(_) => serde_json::from_str(ligne.trim())
                .map(Some)
                .map_err(|e| format!("invalid JSON: {e}")),
            Err(e) => Err(format!("read: {e}")),
        }
    };
    ecrit(1, "hello", serde_json::json!({ "generation": 1 }))?;
    let bonjour = lis(&mut lecteur)?.ok_or("closed before hello")?;
    if let Some(erreur) = bonjour.get("error") {
        return Err(format!("hello refused: {erreur}"));
    }
    ecrit(2, "events.subscribe", serde_json::json!({}))?;
    let reponse = lis(&mut lecteur)?.ok_or("closed before the snapshot")?;
    if let Some(erreur) = reponse.get("error") {
        return Err(format!("events.subscribe refused: {erreur}"));
    }
    let mut pertinence = Pertinence::default();
    pertinence.instantane(
        reponse
            .pointer("/result/terminals")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice),
    );
    tracing::info!("événements herdr-core : abonné");
    recepteur.branche(true);
    recepteur.signe_de_vie();
    // L'instantané peut différer de la dernière lecture : relire une fois.
    recepteur.reveille();
    while !arret.load(Ordering::SeqCst) {
        let Ok(Some(notification)) = lis(&mut lecteur) else {
            // Fermeture, silence de plus de 40 s ou ligne illisible : la
            // session est finie, la boucle se réabonne.
            return Ok(());
        };
        recepteur.signe_de_vie();
        if pertinence.juge(&notification) {
            recepteur.reveille();
        }
    }
    Ok(())
}
