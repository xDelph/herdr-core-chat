//! Transport Unix pour herdr-core (P2a) : JSON-RPC 2.0, une ligne par message,
//! corrélation par identifiant, cadre borné, délai par méthode.
//!
//! ⚠️ **Une connexion par demande** pour les méthodes requête/réponse, comme
//! pour Herdr 0.9.3 (mesuré : l'amont ferme après chaque réponse). La
//! souscription `events.subscribe` garde sa connexion ouverte (flux de
//! notifications) — elle est gérée à part par le port.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CADRE_MAX_OCTETS: usize = 16 * 1024 * 1024;
pub const DELAI_PAR_DEFAUT: Duration = Duration::from_secs(15);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ErreurTransport {
    Ferme,
    CadreTropGrand,
    JsonInvalide(String),
    EnveloppeInvalide(String),
    AmontRefus {
        code: i64,
        message: String,
        kind: Option<String>,
    },
    MauvaiseCorrelation {
        attendu: u64,
        recu: u64,
    },
    DelaiDepasse,
    Socket(String),
}

impl std::fmt::Display for ErreurTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurTransport::Ferme => f.write_str("socket closed by upstream"),
            ErreurTransport::CadreTropGrand => f.write_str("frame beyond 16 MiB"),
            ErreurTransport::JsonInvalide(d) => write!(f, "invalid JSON: {d}"),
            ErreurTransport::EnveloppeInvalide(d) => write!(f, "invalid envelope: {d}"),
            ErreurTransport::AmontRefus {
                code,
                message,
                kind,
            } => {
                write!(f, "upstream refusal {code}: {message}")?;
                if let Some(k) = kind {
                    write!(f, " (kind={k})")?;
                }
                Ok(())
            }
            ErreurTransport::MauvaiseCorrelation { attendu, recu } => {
                write!(f, "correlation broken: expected {attendu}, got {recu}")
            }
            ErreurTransport::DelaiDepasse => f.write_str("timed out without a verdict"),
            ErreurTransport::Socket(d) => write!(f, "socket: {d}"),
        }
    }
}

impl std::error::Error for ErreurTransport {}

pub fn delai_de(methode: &str) -> Duration {
    match methode {
        "terminal.run" => Duration::from_secs(125),
        _ => DELAI_PAR_DEFAUT,
    }
}

#[derive(Debug)]
pub struct TransportCore {
    chemin: PathBuf,
    prochain_id: u64,
}

impl TransportCore {
    pub fn nouveau(chemin: impl AsRef<Path>) -> Self {
        Self {
            chemin: chemin.as_ref().to_path_buf(),
            prochain_id: 1,
        }
    }

    pub fn chemin(&self) -> &Path {
        &self.chemin
    }

    /// Appel requête/réponse. Renvoie le `result` JSON en cas de succès.
    pub fn appelle(&mut self, methode: &str, params: Value) -> Result<Value, ErreurTransport> {
        let id = self.prochain_id;
        self.prochain_id = self.prochain_id.wrapping_add(1);

        let mut stream = UnixStream::connect(&self.chemin)
            .map_err(|e| ErreurTransport::Socket(e.to_string()))?;
        stream
            .set_read_timeout(Some(delai_de(methode)))
            .map_err(|e| ErreurTransport::Socket(e.to_string()))?;
        stream
            .set_write_timeout(Some(delai_de(methode)))
            .map_err(|e| ErreurTransport::Socket(e.to_string()))?;

        let requete = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": methode,
            "params": params,
        });
        let mut ligne = serde_json::to_string(&requete)
            .map_err(|e| ErreurTransport::JsonInvalide(e.to_string()))?;
        ligne.push('\n');
        stream
            .write_all(ligne.as_bytes())
            .map_err(|e| ErreurTransport::Socket(e.to_string()))?;
        stream
            .flush()
            .map_err(|e| ErreurTransport::Socket(e.to_string()))?;

        // Lire une ligne de réponse.
        let mut reader = BufReader::new(stream);
        let mut reponse_ligne = String::new();
        let n = reader
            .read_line(&mut reponse_ligne)
            .map_err(|e| ErreurTransport::Socket(e.to_string()))?;
        if n == 0 {
            return Err(ErreurTransport::Ferme);
        }
        if reponse_ligne.len() > CADRE_MAX_OCTETS {
            return Err(ErreurTransport::CadreTropGrand);
        }
        let reponse: Value = serde_json::from_str(reponse_ligne.trim())
            .map_err(|e| ErreurTransport::JsonInvalide(e.to_string()))?;

        // Vérifier l'id.
        let recu_id = reponse.get("id").and_then(Value::as_u64);
        if let Some(rid) = recu_id {
            if rid != id {
                return Err(ErreurTransport::MauvaiseCorrelation {
                    attendu: id,
                    recu: rid,
                });
            }
        } else {
            return Err(ErreurTransport::EnveloppeInvalide("id absent".into()));
        }

        if let Some(err) = reponse.get("error") {
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let kind = err
                .get("data")
                .and_then(|d| d.get("kind"))
                .and_then(Value::as_str)
                .map(String::from);
            return Err(ErreurTransport::AmontRefus {
                code,
                message,
                kind,
            });
        }

        reponse
            .get("result")
            .cloned()
            .ok_or_else(|| ErreurTransport::EnveloppeInvalide("result absent".into()))
    }
}
