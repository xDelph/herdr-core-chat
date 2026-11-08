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
    /// Quand le mot de statut a changé pour la dernière fois (ms UTC) ;
    /// `None` : le programme n'a jamais rien dit.
    pub status_ms: Option<u64>,
    pub title: String,
    pub labels: std::collections::BTreeMap<String, String>,
    pub parent: Option<String>,
    pub foreground_pid: Option<u32>,
    pub foreground_cwd: Option<String>,
    /// Le nom du programme au premier plan (`foreground.name`).
    pub foreground_name: Option<String>,
    /// L'argv du programme au premier plan (`foreground.argv`).
    pub foreground_argv: Vec<String>,
}

/// La réponse de `terminal.send` (`SendResult`, schéma 0.1.17).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvoiCore {
    /// Le tour au moment de l'insertion.
    pub turn: u64,
    /// Le mot de statut du programme à l'insertion (`null` : il n'a rien dit).
    pub status: Option<String>,
    /// Passé `working` dans les secondes qui suivent ; `None` : programme
    /// sans statut, personne ne peut le dire.
    pub started: Option<bool>,
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

    /// Le socket du core — pour ouvrir un client à part (envoi long).
    #[must_use]
    pub fn chemin(&self) -> &std::path::Path {
        self.transport.chemin()
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
                status_ms: t.get("status_ms").and_then(Value::as_u64),
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
                foreground_name: t
                    .get("foreground")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .map(String::from),
                foreground_argv: t
                    .get("foreground")
                    .and_then(|f| f.get("argv"))
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
        Ok(TerminalList { terminals: out })
    }

    /// `terminal.send` : colle le texte **puis appuie sur Entrée** (il n'existe
    /// pas d'écriture sans Entrée). Refusé (`blocked`) sur un programme
    /// bloqué, sauf `force`.
    pub fn terminal_send(
        &mut self,
        terminal: &str,
        text: &str,
        if_state: Option<&str>,
        force: bool,
    ) -> Result<EnvoiCore, ErreurTransport> {
        let mut params = json!({"terminal": terminal, "text": text});
        if let Some(s) = if_state {
            params["if"] = json!(s);
        }
        if force {
            params["force"] = json!(true);
        }
        let r = self.transport.appelle("terminal.send", params)?;
        let turn = r
            .get("turn")
            .and_then(Value::as_u64)
            .ok_or_else(|| ErreurTransport::EnveloppeInvalide("terminal.send sans turn".into()))?;
        Ok(EnvoiCore {
            turn,
            // `status` est un MOT (`"idle"`), pas l'objet 7501 du terminal.
            status: r.get("status").and_then(Value::as_str).map(String::from),
            started: r.get("started").and_then(Value::as_bool),
        })
    }

    /// Pose des labels (`terminal.label`) ; `clé = ""` en retire un.
    pub fn terminal_label(
        &mut self,
        terminal: &str,
        labels: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), ErreurTransport> {
        self.transport.appelle(
            "terminal.label",
            json!({"terminal": terminal, "labels": labels}),
        )?;
        Ok(())
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

    /// `terminal.run` du **shell de la machine** (ni `argv` ni `line`), dans
    /// `cwd`, avec ses labels — `no_parent`/`no_notify` : herdr-core ne tape
    /// aucune note `<hc>` dans un terminal de l'utilisateur à son sujet.
    /// Rend l'adresse et le PID du shell au premier plan.
    pub fn terminal_run_shell(
        &mut self,
        cwd: &str,
        name: Option<&str>,
        labels: &std::collections::BTreeMap<String, String>,
    ) -> Result<(String, Option<u32>), ErreurTransport> {
        let mut params = json!({
            "cwd": cwd,
            "labels": labels,
            "keep": true,
            "no_parent": true,
            "no_notify": true,
        });
        if let Some(nom) = name {
            params["name"] = json!(nom);
        }
        let r = self.transport.appelle("terminal.run", params)?;
        let adresse = r
            .get("address")
            .and_then(Value::as_str)
            .filter(|adresse| !adresse.is_empty())
            .ok_or_else(|| ErreurTransport::EnveloppeInvalide("terminal.run sans address".into()))?
            .to_string();
        let pid = r
            .get("foreground")
            .and_then(|f| f.get("pid"))
            .and_then(Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok());
        Ok((adresse, pid))
    }

    /// `terminal.kill` : raccroche, termine, puis tue le programme.
    pub fn terminal_kill(&mut self, terminal: &str) -> Result<(), ErreurTransport> {
        self.transport
            .appelle("terminal.kill", json!({"terminal": terminal}))?;
        Ok(())
    }

    /// `terminal.interrupt` : la touche d'arrêt, puis ce qui a été observé
    /// (`stopped`, `exited`, `still_working`, `unknown`). Surveille au plus
    /// 5 s (le transport attend 15 s).
    pub fn terminal_interrupt(&mut self, terminal: &str) -> Result<String, ErreurTransport> {
        let r = self.transport.appelle(
            "terminal.interrupt",
            json!({"terminal": terminal, "timeout": 5}),
        )?;
        r.get("outcome")
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| {
                ErreurTransport::EnveloppeInvalide("terminal.interrupt sans outcome".into())
            })
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
