//! Lecture tolérante de la disposition Herdr.app (`doc` nommé `herdr-app`).
//!
//! ⚠️ Les causes sont **affichées** dans la PWA (espace « layout
//! unavailable ») : elles sont en anglais (AGENTS §1.8).
//!
//! Le format du doc appartient à Herdr.app : on lit ce qu'on connaît, et on
//! renvoie une cause nommée (jamais une reconstruction au hasard) si la
//! structure est inconnue. Les noms affichés viennent du label `where` des
//! terminaux, pas de ce doc (ses `name` sont vides pour les espaces/onglets
//! non nommés).

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Onglet {
    pub id: String,
    pub name: String,
    /// Adresses des terminaux contenus (parcours de l'arbre `root`).
    pub terminaux: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Espace {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub onglets: Vec<Onglet>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disposition {
    pub spaces: Vec<Espace>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErreurDisposition {
    pub detail: String,
}

impl std::fmt::Display for ErreurDisposition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for ErreurDisposition {}

/// Extrait la liste des adresses de terminaux d'un arbre `root`
/// (row/col/block → {terminal}).
pub fn terminaux_de_root(root: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collecter_terminaux(root, &mut out);
    out
}

fn collecter_terminaux(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            if let Some(t) = o.get("terminal").and_then(Value::as_str) {
                out.push(t.to_string());
            }
            if let Some(children) = o.get("children").and_then(Value::as_array) {
                for c in children {
                    collecter_terminaux(c, out);
                }
            }
            // certains nœuds portent "content": { "terminal": "t1" }
            if let Some(content) = o.get("content") {
                collecter_terminaux(content, out);
            }
        }
        Value::Array(arr) => {
            for c in arr {
                collecter_terminaux(c, out);
            }
        }
        _ => {}
    }
}

/// Parse tolérant. Renvoie `Err` si la structure est inconnue — jamais de
/// reconstruction au hasard.
pub fn disposition_depuis(value: &Value) -> Result<Disposition, ErreurDisposition> {
    let spaces_val = value.get("spaces").ok_or_else(|| ErreurDisposition {
        detail: "no `spaces` field".into(),
    })?;
    let spaces_arr = spaces_val.as_array().ok_or_else(|| ErreurDisposition {
        detail: "`spaces` is not an array".into(),
    })?;

    let mut spaces = Vec::new();
    for (i, sp) in spaces_arr.iter().enumerate() {
        let id = sp
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ErreurDisposition {
                detail: format!("space #{i} has no `id`"),
            })?
            .to_string();
        let name = sp
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let folder = sp
            .get("folder")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let tabs_val = sp.get("tabs").ok_or_else(|| ErreurDisposition {
            detail: format!("space {id} has no `tabs`"),
        })?;
        let tabs_arr = tabs_val.as_array().ok_or_else(|| ErreurDisposition {
            detail: format!("`tabs` of {id} is not an array"),
        })?;

        let mut onglets = Vec::new();
        for (j, tb) in tabs_arr.iter().enumerate() {
            let tid = tb
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| ErreurDisposition {
                    detail: format!("tab #{j} of {id} has no `id`"),
                })?
                .to_string();
            let tname = tb
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let terminaux = tb.get("root").map(terminaux_de_root).unwrap_or_default();
            onglets.push(Onglet {
                id: tid,
                name: tname,
                terminaux,
            });
        }

        spaces.push(Espace {
            id,
            name,
            folder,
            onglets,
        });
    }

    Ok(Disposition { spaces })
}
