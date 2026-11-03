//! Les agents installés — la source de `integration.list` sous herdr-core.
//!
//! Herdr 0.9.3 publiait `integration.list` ; herdr-core n'a pas d'équivalent.
//! Le démon cherche donc chaque CLI d'agent connue dans le `PATH` **de sa
//! configuration** (le service le fixe, voir la formule) et rend la forme
//! que `catalog::port::installees_depuis` lit déjà :
//! `{integrations: [{available, command, label, state, target}]}`.
//!
//! ⚠️ Un exécutable trouvé n'est qu'« installé » : le lancement passe ensuite
//! par le shell de l'utilisateur, qui peut résoudre autrement (alias, autre
//! `PATH`). Un agent absent est `not_installed`, jamais omis en silence.

use serde_json::{Value, json};
use std::path::Path;

/// Les CLI d'agents connues : (commande, cible/kind, libellé).
pub const AGENTS: [(&str, &str, &str); 6] = [
    ("pi", "pi", "Pi"),
    ("claude", "claude", "Claude Code"),
    ("codex", "codex", "Codex"),
    ("opencode", "opencode", "opencode"),
    ("devin", "devin", "Devin"),
    ("cursor-agent", "cursor", "Cursor"),
];

/// Un fichier exécutable de ce nom dans un des dossiers du `PATH`.
fn trouve(commande: &str, path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.split(':')
        .filter(|dossier| !dossier.is_empty())
        .map(|dossier| Path::new(dossier).join(commande))
        .any(|chemin| {
            std::fs::metadata(&chemin)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

/// La liste des agents, installés ou non, d'après `path`.
#[must_use]
pub fn integrations_par_path(path: &str) -> Value {
    let liste: Vec<Value> = AGENTS
        .iter()
        .map(|(commande, cible, libelle)| {
            let installe = trouve(commande, path);
            json!({
                "available": installe,
                "command": commande,
                "label": libelle,
                "state": if installe { "current" } else { "not_installed" },
                "target": cible,
            })
        })
        .collect();
    json!({ "integrations": liste })
}
