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
    pub foreground_pid: Option<u32>,
    pub foreground_cwd: Option<String>,
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
                foreground_pid: t
                    .get("foreground")
                    .and_then(|f| f.get("pid"))
                    .and_then(Value::as_u64)
                    .map(|p| p as u32),
                foreground_cwd: t
                    .get("foreground")
                    .and_then(|f| f.get("cwd"))
                    .and_then(Value::as_str)
                    .map(String::from),
            });
        }
        Ok(TerminalList { terminals: out })
    }

    pub fn terminal_send(
        &mut self,
        terminal: &str,
        text: &str,
        if_state: Option<&str>,
    ) -> Result<(u64, Option<String>, Option<bool>), ErreurTransport> {
        let mut params = json!({"terminal": terminal, "text": text});
        if let Some(s) = if_state {
            params["if"] = json!(s);
        }
        let r = self.transport.appelle("terminal.send", params)?;
        let turn = r.get("turn").and_then(Value::as_u64).unwrap_or(0);
        let status = r.get("status").and_then(|v| {
            if v.is_null() {
                None
            } else {
                v.get("state").and_then(Value::as_str).map(String::from)
            }
        });
        let started = r.get("started").and_then(Value::as_bool);
        Ok((turn, status, started))
    }

    pub fn terminal_keys(
        &mut self,
        terminal: &str,
        keys: &[String],
    ) -> Result<(), ErreurTransport> {
        self.transport
            .appelle("terminal.keys", json!({"terminal": terminal, "keys": keys}))?;
        Ok(())
    }

    pub fn terminal_run(
        &mut self,
        argv: &[String],
        cwd: Option<&str>,
        name: Option<&str>,
        labels: &std::collections::BTreeMap<String, String>,
        no_parent: bool,
        no_notify: bool,
    ) -> Result<String, ErreurTransport> {
        let mut params = json!({"argv": argv});
        if let Some(c) = cwd {
            params["cwd"] = json!(c);
        }
        if let Some(n) = name {
            params["name"] = json!(n);
        }
        if !labels.is_empty() {
            params["labels"] = json!(labels);
        }
        params["no_parent"] = json!(no_parent);
        params["no_notify"] = json!(no_notify);
        let r = self.transport.appelle("terminal.run", params)?;
        Ok(r.get("address")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    pub fn terminal_interrupt(&mut self, terminal: &str) -> Result<String, ErreurTransport> {
        let r = self
            .transport
            .appelle("terminal.interrupt", json!({"terminal": terminal}))?;
        Ok(r.get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string())
    }

    pub fn terminal_read(&mut self, terminal: &str, lines: u32) -> Result<String, ErreurTransport> {
        let r = self.transport.appelle(
            "terminal.read",
            json!({"terminal": terminal, "lines": lines}),
        )?;
        Ok(r.get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    pub fn doc_get(&mut self, name: &str) -> Result<DocResult, ErreurTransport> {
        let r = self.transport.appelle("doc.get", json!({"name": name}))?;
        Ok(DocResult {
            value: r.get("value").cloned().filter(|v| !v.is_null()),
            version: r.get("version").and_then(Value::as_u64).unwrap_or(0),
        })
    }
}
