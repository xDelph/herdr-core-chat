//! Client typé herdr-core (P2a) : méthodes minimales pour la lecture.

use crate::transport::{ErreurTransport, TransportCore};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct CoreInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct HelloResult {
    pub generation: u32,
    pub version: String,
    pub features: Vec<String>,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Terminal {
    pub address: String,
    pub name: Option<String>,
    pub argv: Vec<String>,
    pub cwd: String,
    pub state: String,
    pub status: Option<Value>,
    pub turn: u64,
    pub title: String,
    pub labels: std::collections::BTreeMap<String, String>,
    pub parent: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TerminalList {
    pub terminals: Vec<Terminal>,
}

#[derive(Clone, Debug)]
pub struct DocResult {
    pub value: Option<Value>,
    pub version: u64,
}

pub struct ClientCore {
    transport: TransportCore,
}

impl ClientCore {
    pub fn nouveau(chemin: impl AsRef<std::path::Path>) -> Self {
        Self {
            transport: TransportCore::nouveau(chemin),
        }
    }

    pub fn hello(&mut self) -> Result<HelloResult, ErreurTransport> {
        let r = self.transport.appelle("hello", json!({"generation": 1}))?;
        Ok(HelloResult {
            generation: r.get("generation").and_then(Value::as_u64).unwrap_or(0) as u32,
            version: r
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            features: r
                .get("features")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            methods: r
                .get("methods")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn server_status(&mut self) -> Result<CoreInfo, ErreurTransport> {
        let r = self.transport.appelle("server.status", json!({}))?;
        let core = r.get("core").cloned().unwrap_or(Value::Null);
        Ok(CoreInfo {
            id: core
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            name: core
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    pub fn terminal_list(&mut self) -> Result<TerminalList, ErreurTransport> {
        let r = self.transport.appelle("terminal.list", json!({}))?;
        let terms = r
            .get("terminals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::new();
        for t in terms {
            out.push(Terminal {
                address: t
                    .get("address")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                name: t.get("name").and_then(Value::as_str).map(String::from),
                argv: t
                    .get("argv")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                cwd: t
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                state: t
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                status: t.get("status").cloned(),
                turn: t.get("turn").and_then(Value::as_u64).unwrap_or(0),
                title: t
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                labels: t
                    .get("labels")
                    .and_then(Value::as_object)
                    .map(|o| {
                        o.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                parent: t.get("parent").and_then(Value::as_str).map(String::from),
            });
        }
        Ok(TerminalList { terminals: out })
    }

    pub fn doc_get(&mut self, name: &str) -> Result<DocResult, ErreurTransport> {
        let r = self.transport.appelle("doc.get", json!({"name": name}))?;
        Ok(DocResult {
            value: r.get("value").cloned().filter(|v| !v.is_null()),
            version: r.get("version").and_then(Value::as_u64).unwrap_or(0),
        })
    }
}
