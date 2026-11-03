# herdr-core-chat — PROTOCOLE (vérité terrain)

> Document de travail. **Chaque affirmation est étiquetée par son mode de preuve.**
>
> - `[EXÉCUTÉ]` — observé en lançant la commande, sortie réelle (core isolé `fc5d7ddcff43920969b7fc325fe49f34`, herdr-core 0.1.17)
> - `[SCHÉMA]` — lu dans le schéma OpenRPC (`hc schema` / `rpc.discover`)
> - `[INCONNU]` — **non vérifié**, ne pas supposer
>
> Relevé du **2026-11-03**. `hc` / herdr-core **0.1.17** · herdr-app **0.1.12**.
> Core de test isolé : `HC_SESSION=hcc-test-1793715856`, `XDG_STATE_HOME=/tmp/scratchpad/hcc-state`,
> id `fc5d7ddcff43920969b7fc325fe49f34` (≠ id utilisateur `a65ebc791dce6142cc59e1426bd78292`).

---

## 0. Conclusions qui changent le plan

1. **`events.subscribe` livre un instantané + `seq`, puis des notifications typées** (`created`, `lost`, `restarted`, `keep`, `finished`, `removed`, `status`, `turn`, `title`, `labels`, `size`, `doc`). Vérifié : un `terminal.label` produit `{"type":"labels",...}` sur la connexion. `[EXÉCUTÉ]`
2. **`turn` est incrémenté à chaque sortie de `working`** (champ `turn` du terminal) et sert de clé de déduplication. `[SCHÉMA]` (champ `turn` décrit « Goes up each time the status leaves working »)
3. **herdr-core n'expose aucun lien vers le fichier de session** (pas d'équivalent d'`agent_session` 0.9.3). Résolution par agent obligatoire. `[EXÉCUTÉ]` (champ absent du schéma `Terminal`)
4. **Un shell sans agent n'a pas de `status` (`null`)** et `terminal.send` avec `if: idle` répond `status_mismatch` dans ce cas. `[EXÉCUTÉ]`
5. **`doc.get herdr-app` renvoie `null`/version 0** tant que Herdr.app n'a pas écrit sa disposition — état vide normal, pas une erreur. `[EXÉCUTÉ]`

---

## 1. Transport

### 1.1 Endpoint `[EXÉCUTÉ]`

- Socket par session : `~/.local/state/herdr-core/<session>/<session>.sock`
  (ex. `~/.local/state/herdr-core/default/default.sock`).
- `HC_SESSION` choisit la session. `[SCHÉMA]` (help `hc`)
- JSON-RPC 2.0, un objet JSON par ligne. `[EXÉCUTÉ]` (hello round-trip OK)
- Mode « framed » après `hello {framed:true}` (exigé par `terminal.attach`). `[SCHÉMA]`

### 1.2 Handshake `[EXÉCUTÉ]`

`hello {generation: 1}` → `generation` (1), `version` (« 0.1.17 »), `methods[]`,
`features[]` (framed, attach, events, status, shell, resize_notifications), `framed`.

Refuser une génération inconnue : erreur `unsupported_generation` (`data.kind`). `[SCHÉMA]`

### 1.3 Erreurs `[SCHÉMA]`

`data.kind` stable : `not_found`, `blocked`, `status_mismatch`, `finished`,
`unavailable`, `invalid_params`, `name_taken`, `spawn_failed`, `machine_unknown`,
`config`, `conflict`, `internal`, `live`, `needs_target`, `needs_framed`,
`needs_update`, `edit_failed`, `not_text`, `io`, `timeout`…

---

## 2. Terminaux et statut

### 2.1 Forme d'un terminal `[EXÉCUTÉ]`

Champs observés : `address` (`t1`, ou `machine/t1`), `name`, `argv`, `cwd`,
`foreground {pid,name,argv,cwd}`, `labels`, `parent`, `state`
(`live`/`finished`), `status`, `turn`, `title`, `exit_code`, `reason`,
`created_ms`, `cols`, `rows`, `identity`, `keep`, `notify`, `pid`, `reported`,
`records`, `screen`, `status_ms`, `stop`, `submit`, `template`, `signal`…

### 2.2 Statut (OSC 7501) `[EXÉCUTÉ]`/`[SCHÉMA]`

`status` = `{app, state, msg?, title?, kind?, progress?}`.
États : `idle`, `working`, `done`, `blocked`, `error`.
`kind` ∈ `permission|question|auth` quand `blocked`. `[SCHÉMA]`
**Pi sous herdr-core : inconnu** (aucun Pi lancé sur le core de test). `[INCONNU]`

### 2.3 `turn` `[SCHÉMA]`

Augmente à chaque sortie de `working`. Clé naturelle pour « fin de tour » et
pour la déduplication. `[SCHÉMA]` (description du champ)

### 2.4 Shell sans agent `[EXÉCUTÉ]`

`status` vaut `null`. Vérifié : `hc run sh` → `status: null` dans `hc get`.

---

## 3. Événements

### 3.1 `events.subscribe` `[EXÉCUTÉ]`/`[SCHÉMA]`

Paramètres : `labels?`, `terminal?`, `children_of?`.
Résultat : instantané (`terminals[]`, `seq`) + `channel` (null en mode plain,
un numéro en mode framed).

Puis chaque changement arrive en notification `event` :
`created`, `lost`, `restarted`, `keep`, `finished`, `removed`, `status`,
`turn`, `title`, `labels`, `size`, `doc`.
`heartbeat` toutes les 15 s. `[SCHÉMA]`
`missed` + nouvel instantané si retard. `[SCHÉMA]` (non exercé sur le core
de test — à vérifier en charge)

### 3.2 Vérification `[EXÉCUTÉ]`

- `terminal.label` produit bien `{"type":"labels", "seq":N, "terminal":"t1", "data":{"labels":{...}}}`. `[EXÉCUTÉ]`
- L'instantané initial contient `seq` cohérent avec les événements suivants. `[EXÉCUTÉ]`

---

## 4. Actions

### 4.1 `terminal.send` `[EXÉCUTÉ]`/`[SCHÉMA]`

`{text, if?: idle|working, force?, wait?}`.
- Colle + Entrée quand personne n'a tapé depuis ~3 s.
- Refuse un `blocked` sans `force` (`data.kind = blocked`). `[SCHÉMA]`
- Renvoie `turn`, `status` au moment de l'insertion, `started` (passé
  `working` ou non ; `null` = indéterminable). `[SCHÉMA]`
- Vérifié : sur un `sh` (status null), `if: idle` → `status_mismatch`. `[EXÉCUTÉ]`
- Vérifié : `started` vaut `null` pour un programme sans statut. `[EXÉCUTÉ]`

### 4.2 `terminal.keys` `[SCHÉMA]`

Touches nommées (`enter`, `esc`, `tab`, `up`, `ctrl+c`, …).

### 4.3 `terminal.interrupt` `[EXÉCUTÉ]`/`[SCHÉMA]`

→ `outcome`: `stopped|exited|still_working|unknown`. Plus `key` (ex. `ctrl+c`).
Vérifié : sur `sleep 100`, `interrupt` → `outcome: exited`, état `finished`. `[EXÉCUTÉ]`

### 4.4 `terminal.put` `[EXÉCUTÉ]`/`[SCHÉMA]`

Fichier ≤ 8 Mo à côté du terminal, renvoie le chemin, ne tape rien.
Vérifié : 5 octets → `path: …/files/t1/test.txt`. `[EXÉCUTÉ]`

### 4.5 `terminal.read` `[EXÉCUTÉ]`/`[SCHÉMA]`

`{lines}` → `text`, `complete`, `how`, `why` (`working|blocked|error` :
écran seul). `[SCHÉMA]`

### 4.6 `terminal.run` `[SCHÉMA]`

`{argv|line, cwd, name, labels, env, no_parent, no_notify, outlive_parent, keep}`.
Templates : `hc templates` (vide au relevé). `[EXÉCUTÉ]`

⚠️ hc tape des notes `<hc from=…>` dans l'entrée des agents parents (rapports
d'enfants). Les reconnaître au rendu ; lancer nos propres agents avec
`no_parent` / `no_notify`. `[SCHÉMA]`

---

## 5. Documents partagés

### 5.1 `doc.get` / `doc.set` `[EXÉCUTÉ]`/`[SCHÉMA]`

JSON opaque ≤ 1 Mio, écriture conditionnelle par version, événement `doc`.
Vérifié : `doc.get {name: "herdr-app"}` → `{value: null, version: 0}` sur core
vierge. `[EXÉCUTÉ]`

### 5.2 Doc `herdr-app` `[EXÉCUTÉ]` (lu dans le fichier d'état du core)

`spaces[] {id, name, folder, tabs[] {id, name, root: arbre row/col/block →
{terminal}}}`. Les `name` sont vides pour les espaces/onglets non nommés.

Labels posés par l'app : `herdr.space`, `herdr.tab`, `herdr.seen`,
`herdr.pinned`, `where` (= nom affiché). Un terminal sans `herdr.space`
n'est pas affiché par l'app. `[EXÉCUTÉ]`

---

## 6. Sessions (le contenu)

herdr-core **n'expose aucun lien vers le fichier de session** (pas d'équivalent
d'`agent_session`). `[EXÉCUTÉ]` (champ absent du schéma `Terminal`)

Résolution **par agent**, sans repli d'un profil vers un autre :
- **Claude Code** : `foreground.pid` → `~/.claude/sessions/<pid>.json` →
  `sessionId` (+ `cwd`). `[EXÉCUTÉ]` (sur 0.9.3 ; à confirmer sous herdr-core)
- **Pi, opencode, codex, devin** : à trouver et prouver. Tant que ce n'est pas
  prouvé : agent affiché « sans fil », jamais un fil deviné. `[INCONNU]`

---

## 7. Ce qui reste à prouver (P1)

- `events.subscribe` : comportement `missed` + resnapshot sous charge. `[INCONNU]`
- `terminal.send` avec `blocked` (refus) et `force`. `[INCONNU]` (pas d'agent
  bloqué sous le core de test)
- `terminal.read` : champs `complete`/`how`/`why`. `[SCHÉMA]`
- OSC 7501 sous herdr-core pour Pi. `[INCONNU]`
- Résolution de session pour Pi/opencode/codex/devin. `[INCONNU]`