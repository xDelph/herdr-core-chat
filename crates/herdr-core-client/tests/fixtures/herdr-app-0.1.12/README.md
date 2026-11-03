# Fixtures Herdr.app 0.1.12 / herdr-core 0.1.17

Relevées le 2026-11-03, en lecture seule, puis anonymisées : dossiers et
adresses de terminaux remplacés, noms vides gardés vides.

- `doc-herdr-app.json` : la valeur du doc `herdr-app` (`doc.get`, version 38).
  Forme réelle : `spaces[] {id, name, folder, tabs[] {id, name, root}}`, avec
  `root` = `row` → `children[]` de `block` → `content.terminal`. Tous les
  `name` sont vides : Herdr.app calcule les noms affichés.
- `workspace-state.json` : `~/.config/herdr-app/workspace-state.json` (écrit
  par le mod de Herdr.app), même forme de clés, valeurs anonymes. Ses
  `spaces[].id` / `tabs[].id` sont ceux du doc : c'est la source des noms.

Un changement de format chez Herdr.app doit faire échouer
`tests/unit/contrat_herdr_app.rs` : on ajoute alors une nouvelle fixture
versionnée, on ne retouche pas celle-ci.
