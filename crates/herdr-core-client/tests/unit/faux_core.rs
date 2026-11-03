//! Un faux herdr-core : un socket Unix temporaire qui répond par méthode et
//! garde la trace de chaque demande (méthode + paramètres).

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

static COMPTEUR: AtomicU64 = AtomicU64::new(0);

/// Une réponse du faux core : un `result`, ou une erreur JSON-RPC avec son
/// `data.kind`.
pub(crate) enum Reponse {
    Resultat(Value),
    Erreur { message: String, kind: String },
}

pub(crate) type Demandes = Arc<Mutex<Vec<(String, Value)>>>;

/// Un dossier temporaire propre au test (jamais le HOME).
pub(crate) fn dossier_temporaire(prefixe: &str) -> PathBuf {
    let n = COMPTEUR.fetch_add(1, Ordering::SeqCst);
    let dossier = std::env::temp_dir().join(format!("hcc-{prefixe}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dossier);
    if let Err(cause) = std::fs::create_dir_all(&dossier) {
        panic!("dossier temporaire refusé : {cause}");
    }
    dossier
}

/// Démarre le faux core ; rend le chemin du socket et le journal des
/// demandes. Le fil vit jusqu'à la fin du processus de test.
pub(crate) fn demarre<F>(repond: F) -> (PathBuf, Demandes)
where
    F: Fn(&str, &Value) -> Reponse + Send + 'static,
{
    let socket = dossier_temporaire("core").join("c.sock");
    let ecoute = match UnixListener::bind(&socket) {
        Ok(ecoute) => ecoute,
        Err(cause) => panic!("socket du faux core refusé : {cause}"),
    };
    let demandes: Demandes = Arc::default();
    let trace = Arc::clone(&demandes);
    thread::spawn(move || {
        for flux in ecoute.incoming() {
            let Ok(mut flux) = flux else { return };
            let Ok(lecture) = flux.try_clone() else {
                return;
            };
            let mut ligne = String::new();
            if BufReader::new(lecture).read_line(&mut ligne).is_err() {
                continue;
            }
            let Ok(requete) = serde_json::from_str::<Value>(ligne.trim()) else {
                continue;
            };
            let methode = requete
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let params = requete.get("params").cloned().unwrap_or(Value::Null);
            if let Ok(mut trace) = trace.lock() {
                trace.push((methode.clone(), params.clone()));
            }
            let id = requete.get("id").cloned().unwrap_or(Value::Null);
            let reponse = match repond(&methode, &params) {
                Reponse::Resultat(resultat) => json!({"jsonrpc":"2.0","id":id,"result":resultat}),
                Reponse::Erreur { message, kind } => json!({
                    "jsonrpc":"2.0","id":id,
                    "error":{"code":-32000,"message":message,"data":{"kind":kind}}
                }),
            };
            let _ = flux.write_all(format!("{reponse}\n").as_bytes());
        }
    });
    (socket, demandes)
}

/// `hello` d'un core 0.1.17.
pub(crate) fn hello(generation: u64) -> Value {
    json!({"generation":generation,"version":"0.1.17","features":["events"],"methods":[],
           "framed":false,"core":{"id":"faux","name":"faux"}})
}

/// Un terminal tel que `terminal.list` le rend.
pub(crate) fn terminal(adresse: &str, status: Value, premier_plan: &str, cwd: &str) -> Value {
    json!({"address":adresse,"name":null,"argv":["/bin/zsh"],"cwd":cwd,"state":"live",
           "status":status,"turn":3,"title":"","labels":{},"parent":null,
           "foreground":{"pid":4242,"name":premier_plan,"argv":[premier_plan],"cwd":cwd}})
}

/// Les demandes reçues pour une méthode.
pub(crate) fn demandes_de(demandes: &Demandes, methode: &str) -> Vec<Value> {
    match demandes.lock() {
        Ok(trace) => trace
            .iter()
            .filter(|(m, _)| m == methode)
            .map(|(_, p)| p.clone())
            .collect(),
        Err(_) => panic!("journal des demandes empoisonné"),
    }
}

/// Le futur d'un port bloquant-déguisé : prêt au premier poll, comme les
/// appelants l'exigent.
pub(crate) fn attends<T>(
    mut futur: domain::ports::PortFuture<'_, T>,
) -> Result<T, domain::errors::PortError> {
    let mut contexte = std::task::Context::from_waker(std::task::Waker::noop());
    match futur.as_mut().poll(&mut contexte) {
        std::task::Poll::Ready(valeur) => valeur,
        std::task::Poll::Pending => panic!("le port doit répondre au premier poll"),
    }
}
