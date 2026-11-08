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
- Mesuré le 2026-11-08 (0.1.18, faux agent, core isolé) : la fenêtre de
  `started` est **≈5 s**. Démarrage à 0,5/2/3,5 s → `started: true` (0,7 à
  3,7 s) ; à 5 ou 8 s → `started: false` en 5,2 s. Agent déjà `working` →
  `started: true` en 0,2 s. L'adaptateur relit le terminal 6 s de plus
  (`working` ou tour qui avance = confirmé) et journalise chaque envoi. `[EXÉCUTÉ]`

### 4.1bis « Vu » partagé : label `herdr.seen` `[EXÉCUTÉ]`

Herdr.app note le dernier tour regardé dans le label `herdr.seen = "<turn>"`
du terminal (partagé entre ses fenêtres et son app téléphone). Le core garde
le mot `done` ; Herdr.app l'affiche `idle` quand `herdr.seen >= turn` (son
guide : « done = finished while the user wasn't looking; it becomes idle once
they look »). Relevé sur le core de l'utilisateur, lecture seule : `view.json`
« seen: false » ⇔ `herdr.seen < turn` (t13, t1g), et `status_ms` suit de 10 à
14 ms la dernière activité de la session.
- Mac → téléphone : `done` + `herdr.seen >= turn` → affiché `idle`, et
  `seen_at = status_ms` alimente `readAt`.
- Téléphone → Mac : une lecture (`POST /read`) pose `herdr.seen = turn`
  (`terminal.label`) sur un `done` fini **avant** l'instant lu. Vérifié sur
  core isolé : lecture antérieure → rien ; lecture courante → `herdr.seen=1`.

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
  `sessionId`. `[EXÉCUTÉ]` sous herdr-core 0.1.17 (2026-11-03, lecture seule :
  3 terminaux `claude-code` du core de l'utilisateur, fichier présent pour
  chaque `foreground.pid`, champ `pid` du fichier = ce PID). Le démon exige
  cette égalité (PID recyclé → aucun fil).
- **opencode** : titre de terminal `OC | <titre de session>` + dossier
  (`foreground.cwd`) → **une seule** session principale non archivée de
  `session_v2` (`directory`, `title`, `parent_id IS NULL`). `[EXÉCUTÉ]`
  (lecture seule, 2026-11-03 : `t18`, lancé `opencode -s
  ses_e5e4196c…`, titre `OC | AGENTS.md explanation request` → exactement
  `ses_e5e4196c…` ; `t7` → une session). Deux sessions du même titre dans le
  même dossier → aucun fil.
- **codex** : titre `<nom du fil> | <dossier>` + dossier → fil de son index
  (`threads.name` + `cwd`, rollout existant). `[EXÉCUTÉ]` (lecture seule :
  `t1f` « Test | herdr-core-chat » → un fil `Test` dans ce dossier).
- **Pi** : publie bien OSC 7501 sous herdr-core (`status.app = "pi"`,
  premier plan `node`, argv `pi`) `[EXÉCUTÉ]`, mais aucun lien vers sa
  session : titre `π - <dossier>` seulement, aucun fichier de session ouvert
  (`lsof`), et l'extension `herdr-agent-state.ts` (Herdr 0.9.3) ne parle
  qu'à `HERDR_SOCKET_PATH`. → « sans fil ». `[INCONNU]`
- **devin** : non observé sous herdr-core → « sans fil ». `[INCONNU]`
- « La session la plus récente du dossier » a été retirée pour tous : c'est
  un fil DEVINÉ (deux agents du même dossier liraient le même).
- `cursor-agent` tourne sous `node` (premier plan `node`, argv[0]
  `…/cursor-agent`, pas de statut) : type reconnu par l'argv du premier plan.

---

## 6bis. Relevés complémentaires (2026-11-03, deuxième passe)

### `terminal.send` `[SCHÉMA]`
- `text` : « Pasted, then submitted with Enter » — **il n'existe pas
  d'écriture sans Entrée**. L'adaptateur déclare donc `send_text` comme
  « texte + Entrée » et absorbe l'`enter` que l'appelant envoie juste après
  (sinon un second Entrée validerait une ligne vide ou le choix suivant).
- `SendResult.status` est un **mot** (`"idle"`), pas l'objet 7501 — l'ancien
  client le lisait comme un objet et le perdait toujours.
- `started: true` = passé `working` dans les secondes qui suivent : c'est la
  **seule** confirmation d'un prompt. `false`/`null` → « inconnu ».
- `force` : utilisé **seulement** pour répondre à une question
  (`blocked` + `kind: question`), jamais pour `permission`/`auth`.

### `terminal.interrupt` `[SCHÉMA]`
- `outcome` : `stopped` (sorti de `working`, ou shell revenu à l'invite),
  `exited`, `still_working` (rien vu dans le délai), `unknown` (pas de statut).
  Paramètre `timeout` (10 s par défaut ; l'adaptateur passe 5 s). Dans tous les
  cas la touche est partie : « interruption envoyée », jamais « rien à
  arrêter ».

### `Status` `[SCHÉMA]`
- `state` : « idle, working, done, blocked or error ». `error` a désormais son
  état dans le domaine (`AgentStatus::Error`), sa section (« Needs you ») et sa
  notification (« Agent error »).
- `kind` : « permission, question or auth (blocked only, when the program
  said) ».

### `terminal.list` sur le core de l'utilisateur `[EXÉCUTÉ]` (lecture seule)
- `claude-code` : `foreground.name` vaut la **version** (`2.1.317`), pas
  `claude` — seul `status.app` identifie l'agent.
- Aucun Pi sous herdr-core au relevé ; un terminal `node` sans statut existe
  (programme inconnu) : il reste un pane, pas un agent.

### Disposition Herdr.app `[EXÉCUTÉ]` (lecture seule)
- `doc.get {name: "herdr-app"}` : version 38, 8 espaces, **tous les `name`
  vides**. Arbre : `row` → `children[]` de `block` → `content.terminal` (+ `id`,
  `size.weight`). Fixture anonymisée :
  `crates/herdr-core-client/tests/fixtures/herdr-app-0.1.12/`.
- `~/.config/herdr-app/workspace-state.json` (mod de Herdr.app), `version: 1` :
  `spaces[] {id, name, named, folder, tabs[] {id, name, named, …}}`,
  `agents[]`. Ses identifiants sont **ceux du doc** : c'est la source des noms
  affichés (décision de l'utilisateur, 2026-11-03). Version inconnue → cause
  journalisée, noms du doc.

### Vérification de bout en bout `[EXÉCUTÉ]` (core isolé `57289159…`, 2026-11-03)
- Core jetable : `HC_SESSION=hccv1` + `XDG_STATE_HOME`/`XDG_CONFIG_HOME`/
  `XDG_DATA_HOME`/`XDG_RUNTIME_DIR` temporaires, `HC_TERMINAL_SOCKET` retiré
  de l'environnement. Avec `XDG_RUNTIME_DIR`, le socket est
  `$XDG_RUNTIME_DIR/herdr-core/<session>.sock` (pas sous `XDG_STATE_HOME`).
- Démon (`HOME` temporaire, port 8799) : `/api/health` → `protocol 1`,
  `0.1.17` ; `/api/overview` → l'espace du doc `herdr-app` posé par `doc.set`,
  et le terminal hors disposition groupé « <dossier> (outside Herdr.app) ».
  Doc rendu invalide (`spaces: "cassé"`) → un espace « Herdr.app layout
  unavailable: doc `herdr-app` v2: `spaces` is not an array » + une ligne de
  journal. Données écrites dans `$HOME/.herdr-core-chat/` uniquement.
- Démon puis core arrêtés après usage (`server.status` vérifié avant).

### Lancement (P7) `[EXÉCUTÉ]` (core isolé `4b4d0cb2…`, 2026-11-03)
- Comme `tab.new` de Herdr.app (« Agents run in your interactive shell ») :
  `terminal.run` **sans `argv` ni `line`** (le shell de la machine) avec
  `cwd`, `labels {herdr.space, herdr.tab: tb<32 hex>}`, `keep`, `no_parent`,
  `no_notify` ; puis `terminal.send` de la ligne de l'agent dans CE shell.
- Démon composé, faux `pi` dans le `PATH` (jamais le vrai CLI) :
  `/api/agents/kinds` → `pi` `current` ; `POST /api/workspaces/spl/agents
  {"kind":"pi"}` → 201 `started`, terminal `t2` labellisé, le programme au
  premier plan passe du shell au faux `pi`.
- `start_agent` et `fermer_onglet` refusent tout terminal que le démon n'a
  pas créé. `name` herdr-core : `1-32 of a-z 0-9 - _ .` (nom d'agent
  assaini ; `name_taken` → onglet sans nom).

### Lancement : ce que le terrain a montré (2026-11-08)
- Relevé sur le core de l'utilisateur (lecture seule, `hc read t1q`) : le
  shell neuf affichait « [oh-my-zsh] Would you like to update? [Y/n] » ; la
  ligne `claude --model opus …` tapée y a répondu par son `c`, puis le shell a
  lancé `laude` (« command not found »). Le démon disait « the agent may
  exist » alors que le shell était resté au premier plan. `[EXÉCUTÉ]`
- Correctifs (prouvés sur faux core, `tests/unit/lancement.rs`) :
  - `terminal.run` porte `env {DISABLE_AUTO_UPDATE: "true"}` (variable lue par
    `check_for_upgrade.sh` d'oh-my-zsh quand aucun `zstyle ':omz:update'
    mode` n'est posé), dans CE shell seulement ;
  - une question oui/non en dernière ligne (`[y/n]`, `(y/n)`…) → rien n'est
    tapé (`ShellNotReady`) ;
  - à l'échéance, shell encore au premier plan → `NeverStarted` (la dernière
    ligne de l'écran dans le détail), l'onglet créé est fermé, rien n'est
    rejoué ; premier plan illisible seulement → délai ambigu (onglet gardé) ;
  - terminal disparu de `terminal.list` (un fini n'y est plus) → « the shell
    exited », pas « not on this core ».
- Le label `herdr-core-chat.tab = <onglet>` marque nos terminaux : après un
  redémarrage du démon (mémoire vide), `start_agent`/`fermer_onglet` les
  reconnaissent — jamais par `herdr.tab`, que Herdr.app pose aussi.
- Placement : un terminal absent du doc mais labellisé `herdr.space` (espace
  connu) / `herdr.tab` est rangé dans cet espace et cet onglet, plus dans
  « (outside Herdr.app) ». Le dossier de lancement est `space.folder`, avant
  le `cwd` d'un pane (qui a pu `cd`). Les noms pris comptent les shells nommés.

### Envois, réponses : refus et délais (2026-11-08) `[SCHÉMA]`/tests
- Un refus de `terminal.send`/`terminal.keys` (ex. `blocked` sur une
  permission) est une **erreur** du port : `les_frappes` ne presse jamais
  `enter` après un collage refusé (cet `enter` approuvait la permission).
  Le refus `blocked` est reconnu par `data.kind`, plus par la phrase de
  Herdr 0.9.3.
- Sélecteur de pi : `terminal.keys ["down"|"up"]` (`tui.select.down/up`),
  jamais `j`/`k` collés (`send` colle PUIS appuie sur Entrée).
- Un délai de socket (`WouldBlock`/`TimedOut`) est `DelaiDepasse` →
  `Timeout` → envoi `attente`, réponse `transmise` (plus « rejected »).
- Le port n'a plus de client partagé sous verrou : chaque appel ouvre son
  client (une connexion par appel de toute façon).
- Le « vu » partagé compare `status_ms` à l'horloge du Mac (l'instant de
  réception de `POST /read`), plus à celle du téléphone.

## 7. Ce qui reste à prouver (P1)

- `events.subscribe` : comportement `missed` + resnapshot sous charge. `[INCONNU]`
- `terminal.send` avec `blocked` (refus) et `force`. `[INCONNU]` (pas d'agent
  bloqué sous le core de test) — l'adaptateur suppose `data.kind = blocked`
  (schéma) ; à vérifier sur un faux agent bloqué en core isolé.
- La détection de la demande de confiance **au lancement** (`409
  trust-required`) n'est plus atteinte sous herdr-core : `start_agent` rend
  `Started` dès que le premier plan change ; la confiance se lit ensuite sur
  l'écran de la conversation. `[INCONNU]` (délai d'apparition de l'invite)
- `terminal.read` : champs `complete`/`how`/`why`. `[SCHÉMA]`
- OSC 7501 sous herdr-core pour Pi. `[INCONNU]`
- Résolution de session pour Pi et devin. `[INCONNU]`
- Lancement (P7) : l'**apparition dans Herdr.app** d'un terminal labellisé
  `herdr.space`/`herdr.tab` est documentée par le guide de Herdr.app
  (`~/.config/herdr-app/AGENTS.md` : « a terminal with those labels shows up
  there », « an id Herdr doesn't know becomes a new space or tab ») mais non
  exécutée (aucune fenêtre de test). `[SCHÉMA]`
- `events.subscribe` n'est pas encore utilisé : la veille interroge toutes les
  2 s (`terminal.list` + `doc.get`).