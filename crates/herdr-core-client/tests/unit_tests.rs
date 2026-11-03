//! Cible Cargo externe : les tests de `herdr-core-client`, présentés en
//! sous-modules de `tests/unit` (aucun test sous `src/`).
//!
//! ⚠️ Aucun vrai core : chaque test sert un **faux core** sur un socket
//! temporaire (`unit::faux_core`). Politique clippy héritée : ni `unwrap` ni
//! `expect` — les échecs disent leur motif par `panic!`.

mod unit;
