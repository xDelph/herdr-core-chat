# herdr-core-chat — ce qu'il faut construire

> Fichier de départ, écrit le 2026-11-03 après une investigation en lecture seule.
> Il dit **quoi** faire et **ce qui a été mesuré**. Il ne vaut pas spécification :
> chaque fait marqué `[SCHÉMA]` doit être **vérifié par exécution** avant usage.

## 0. Objectif

Reproduire **Herdr Chat** (`../herdr-pwa`, v0.2.6, commit `dc99436`) pour
**Herdr.app + herdr-core** (`hc`), en **application distincte**.

- Herdr Chat reste l'application du **TUI Herdr 0.9.3** (`~/.config/herdr/herdr.sock`).
  On n'y touche pas : `../herdr-pwa` est une **référence en lecture seule**.
- herdr-core-chat ne parle **qu'à herdr-core / Herdr.app**. Aucun repli vers le
  socket Herdr 0.9.3, aucun mode fusionné.
- Les deux applications doivent pouvoir **tourner en même temps** sur la même
  machine (l'utilisateur migre progressivement).

Ce que l'utilisateur doit retrouver (parité avec le README d'Herdr Chat) :
conversations rendues depuis les sessions des agents (pas le terminal), push
« terminé » / « needs you », questions avec leurs choix en boutons, envoyer /
répondre / arrêter / changer de modèle, rien affiché « envoyé » sans
confirmation, accès par Tailscale Serve uniquement (bind `127.0.0.1`).

## 1. Règles non négociables

Reprises de `../herdr-pwa/AGENTS.md` — les lire en entier, elles viennent
d'incidents réels.

1. **Un terminal de l'utilisateur n'est JAMAIS une cible de test.** `hc ls`
   montre tout son travail en cours. Aucun `send`, `keys`, `interrupt`, `kill`,
   `rm`, `label`, `keep`, `put` sur un terminal que tu n'as pas créé.
2. **Tests contre herdr-core = un core isolé** : `HC_SESSION=<jetable>` + chemins
   XDG temporaires (`XDG_STATE_HOME`, `XDG_CONFIG_HOME`…). Vérifier par
   `server.status` que l'`id` du core n'est **pas** celui de l'utilisateur
   avant le moindre appel en écriture. Arrêter ce core après usage.
3. **Jamais** `hc server stop|restart`, `hc update`, ni arrêt/redémarrage de
   Herdr.app sur le core ou la fenêtre de l'utilisateur. Pas d'automatisation
   clavier/souris sur son écran. `herdr-app do …` (crée des espaces/onglets
   visibles) : interdit hors core/fenêtre de test.
4. **Zéro `kill`** sur un processus non identifié positivement (chemin, PPID,
   service).
5. **Le schéma n'est pas la source de vérité** : toute capacité est vérifiée
   par exécution (sur core isolé) avant d'être utilisée. Leçon d'Herdr 0.9.3 :
   19 variantes déclarées, 1 implémentée.
6. **Un contrat, un seul endroit** (DTO HTTP partagés serveur/PWA + test de
   contrat). **Ne jamais afficher un état non vérifié**, **ne jamais masquer
   une erreur** (cause réelle affichée ; une porte de vérification doit être
   prouvée capable d'échouer).
7. **Aucun test sous `src/`** (Rust comme TypeScript) ; tests dans `tests/`.
   Tests de contrat/scénarios : ports simulés, magasins temporaires, aucun
   vrai CLI d'agent, aucune clé VAPID ni abonnement réel, aucun store
   utilisateur.
8. **UI en anglais** (PWA, notifications, erreurs visibles) ; commentaires et
   specs en français.
9. **Pas de service, Homebrew, Tailscale, ni lancement de daemon de
   production** sans instruction explicite de l'utilisateur. Distribution
   cible : formule brew + `brew services` (pas de launchd fait main).
10. Exécuter seulement les tâches confiées ; préparer un plan n'autorise pas à
    l'implémenter.

## 2. Ce qui se réutilise depuis `../herdr-pwa`

Architecture Rust : un daemon, des crates par responsabilité (voir
`specs/rust-backend/TECH.md`, `ADAPTERS.md`, `CONTRACTS.md`).

| Crate / dossier | Rôle | Pour herdr-core-chat |
|---|---|---|
| `domain` | types, politiques, ports (`ControlPort` `src/ports.rs:240`, `SessionPort`…) | à reprendre ; **vocabulaire à revoir** (voir §4) |
| `sessions` | résolution + lecture des sessions Pi/Claude/codex/opencode/devin | réutilisable tel quel ; la **résolution** change (§3.6) |
| `interactions` | questions lues à l'écran, réponse | réutilisable ; `blocked.kind` aide (§3.2) |
| `notifications` | push VAPID, store | réutilisable ; magasin et clés **séparés** |
| `catalog`, `files`, `contracts`, `http-api`, `application` | catalogue de lancement, fichiers, DTO, routeur, cas d'usage | réutilisables, adapter aux nouvelles observations |
| `herdr-client` | transport + port pour Herdr 0.9.3 | **remplacé** par un adaptateur herdr-core |
| `apps/web`, `packages/shared` | PWA + contrat partagé | réutilisables |
| `docs/PROTOCOL.md`, `DECISIONS.md` | vérités terrain 0.9.3 | modèle de méthode ; refaire l'équivalent pour herdr-core |

Décision à prendre en P0 : **copie** du code à `dc99436` (divergence assumée)
ou dépendance partagée. Recommandé : copie — les deux produits suivent des
amonts qui évoluent à des rythmes différents.

## 3. Faits relevés le 2026-11-03

`hc` / herdr-core **0.1.17** · herdr-app **0.1.12** · Claude Code 2.1.317.
Étiquettes : `[EXÉCUTÉ]` observé en lecture seule ; `[SCHÉMA]` lu dans
`hc schema` (OpenRPC) ou `hc --skill` — **à vérifier**.

Sources à relire d'abord : `hc --help`, `hc --skill`, `hc schema` (ou
`rpc.discover`), `herdr-app --skill`, `herdr-app commands`,
`~/.config/herdr-app/AGENTS.md` (guide installé, généré, ne pas éditer).

### 3.1 Transport
- Socket : `~/.local/state/herdr-core/default/default.sock` `[EXÉCUTÉ]` (fichier
  présent ; `HC_SESSION` choisit une autre session). JSON-RPC 2.0, un message
  par ligne ; mode « framed » après `hello {framed:true}` (exigé par
  `terminal.attach`) `[SCHÉMA]`.
- `hello` → `generation` (1), `version`, `methods[]`, `features[]` : négocier
  au démarrage, refuser une génération inconnue (`unsupported_generation`)
  `[SCHÉMA]`.
- Erreurs : `data.kind` stable (`not_found`, `blocked`, `status_mismatch`,
  `finished`, `unavailable`…) `[SCHÉMA]`.

### 3.2 Terminaux et statut
- Un terminal : `address` (`t7`, ou `machine/t7`), `name`, `argv`, `cwd`,
  `foreground {pid,name,argv,cwd}`, `labels`, `parent`, `state`
  (`live`/`finished`), `status`, `turn`, `title`, `exit_code`, `reason`…
  `[EXÉCUTÉ]` (`hc get`).
- `status` vient du programme lui-même (OSC 7501) : `{app, state, msg?, title?,
  kind?, progress?}`. États : `idle`, `working`, `done`, `blocked`, `error`.
  Observé : `claude-code` `working` avec `msg` = l'action en cours, `opencode`
  `idle` `[EXÉCUTÉ]`. `kind` ∈ `permission|question|auth` quand `blocked`
  `[SCHÉMA]`. **Pi : inconnu** sous herdr-core (aucun Pi lancé là au relevé).
- `turn` augmente à chaque sortie de `working` `[SCHÉMA]` — clé naturelle pour
  « fin de tour » et pour la déduplication.
- Un shell sans agent n'a pas de `status` (`null`) `[EXÉCUTÉ]`.

### 3.3 Événements
- `events.subscribe {labels?, terminal?, children_of?}` → instantané + `seq`,
  puis chaque changement (created, lost, restarted, keep, finished, removed,
  status, turn, title, labels, size, **doc**) ; `heartbeat` /15 s ; `missed` +
  nouvel instantané si on prend du retard `[SCHÉMA]`. À prouver en premier
  (c'est ce qui manquait à 0.9.3 : `revision` toujours 0).

### 3.4 Action
- `terminal.send {text, if?: idle|working, force?, wait?}` : colle + Entrée
  quand personne n'a tapé depuis ~3 s ; refuse un `blocked` sans `force` ;
  renvoie `turn`, `status` au moment de l'insertion, `started` (passé
  `working` ou non ; `null` = indéterminable) `[SCHÉMA]`. Base de la
  confirmation d'envoi — ne jamais forcer une décision de permission.
- `terminal.keys` (touches nommées), `terminal.interrupt` (→ `outcome` :
  `stopped|exited|still_working|unknown`), `terminal.put` (fichier ≤ 8 Mo à côté
  du terminal, renvoie le chemin, ne tape rien) `[SCHÉMA]`.
- `terminal.read {lines}` → `text`, `complete`, `how`, `why` (`working|blocked|
  error` : écran seul) `[SCHÉMA]`.
- `terminal.run {argv|line, cwd, name, labels, env, no_parent, no_notify,
  outlive_parent, keep}` ; templates (`hc templates`, vide au relevé)
  `[SCHÉMA]`.
- ⚠️ hc **tape des notes** `<hc from=…>` dans l'entrée des agents parents
  (rapports d'enfants). Elles peuvent apparaître dans les transcriptions : les
  reconnaître au rendu ; lancer nos propres agents avec `no_parent` /
  `no_notify` `[SCHÉMA]`.

### 3.5 Documents partagés
- `doc.get {name}` / `doc.set {name, value, version}` : JSON opaque ≤ 1 Mio,
  écriture conditionnelle par version, événement `doc` `[SCHÉMA]`.
- Herdr.app y range sa disposition : doc **`herdr-app`** (version 27 au relevé)
  = `spaces[] {id, name, folder, tabs[] {id, name, root: arbre row/col/block →
  {terminal}}}` `[EXÉCUTÉ]` (lu dans le fichier d'état du core). Les `name` sont
  **vides** pour les espaces/onglets non nommés : Herdr calcule alors un nom
  par défaut.
- Labels posés par l'app sur ses terminaux : `herdr.space`, `herdr.tab`,
  `herdr.seen`, `herdr.pinned`, `where` (= nom affiché, ex. `pi-lab / 1`)
  `[EXÉCUTÉ]`. Un terminal sans `herdr.space` n'est pas affiché par l'app.
- « Vu » et « épinglé » sont partagés entre fenêtres et l'app téléphone
  (changelog 0.1.12) : on peut s'y aligner plutôt que d'inventer les nôtres.

### 3.6 Sessions (le contenu)
- herdr-core **n'expose aucun lien vers le fichier de session** (0.9.3 donnait
  `agent_session`). Résolution **par agent**, sans repli d'un profil vers un
  autre :
  - Claude Code : `foreground.pid` → `~/.claude/sessions/<pid>.json` →
    `sessionId` (+ `cwd`) `[EXÉCUTÉ]`.
  - Pi, opencode, codex, devin : **à trouver et prouver**. Tant que ce n'est
    pas prouvé : agent affiché « sans fil », jamais un fil deviné.

## 4. Disposition et noms : lecture « statique », robuste à l'évolution d'Herdr.app

Herdr.app est évolutif (mods, layouts, réglages réécrits à chaque version) :
ne **jamais** dépendre de son rendu, de ses mods ni de ses fichiers privés
(`~/.config/herdr-app/view.json`, `shared.json`, `data/`…).

Sources, par ordre de préférence :
1. **Core seul, sans fenêtre** : doc `herdr-app` (structure espaces → onglets →
   terminaux, dossiers) + label `where` (nom affiché) + labels `herdr.*`. Suivi
   en direct par l'événement `doc` et `labels`. Fonctionne Herdr.app fermé.
2. **`herdr-app ls`** (socket de la fenêtre) : noms calculés, focus — seulement
   si la fenêtre tourne ; information en plus, jamais requise.

Exigences :
- Le format du doc `herdr-app` appartient à Herdr.app : lecture tolérante,
  **fixture versionnée + test de contrat** ; format inconnu → disposition
  affichée « unavailable » avec la cause, jamais reconstruite au hasard.
- Ne pas recoder la règle de nommage par défaut d'Herdr sans l'avoir vérifiée ;
  préférer `where`.
- Un terminal hors disposition (sans `herdr.space`, ex. sous-agent `hc run`)
  reste visible, groupé par `cwd` / `parent`.
- Vocabulaire du domaine : espaces / onglets / terminaux (adresse `t7`), pas
  workspaces / panes `w2:p1`. Familles d'identifiants distinctes (voir
  `../herdr-pwa/crates/domain/src/ids.rs`).

## 5. Coexistence avec Herdr Chat (à confirmer avec l'utilisateur)

Tout doit différer d'Herdr Chat : nom de binaire, port local (Herdr Chat :
`8791`), formule brew et service, répertoire de données, store push et clés
VAPID, port HTTPS Tailscale Serve, nom/icone de la PWA.

## 6. Plan par phases

Chaque phase livre : fichiers, tests, preuves (sortie réelle), et ce qui
reste inconnu.

- **P0 — Socle.** Dépôt, copie depuis `../herdr-pwa`, renommages, portes
  qualité (fmt, clippy, tests, test de contrat), arborescence `tests/`.
  Retirer `herdr-client`.
- **P1 — Vérité terrain herdr-core.** Sur core isolé : `hello`, `terminal.list`,
  `events.subscribe` (y compris `missed`), `doc.get herdr-app`, `send` avec
  `if`/`blocked`, `interrupt`, `put`, `read`. Écrire `docs/PROTOCOL.md`
  (même méthode que 0.9.3 : `[EXÉCUTÉ]`/`[SCHÉMA]`/`[INCONNU]`).
- **P2 — Adaptateur lecture seule.** Transport JSON-RPC, port de contrôle
  (observations), disposition §4, faux core pour les tests. Vue d'accueil
  alimentée par les événements, sans polling.
- **P3 — Résolution des sessions par agent.** Claude d'abord (prouvé), puis
  chaque agent avec sa preuve. Rendu des notes `<hc>`.
- **P4 — Notifications.** `done`, `blocked` (+ `kind` → « needs you »), `error`,
  fin de terminal ; déduplication par `turn`.
- **P5 — Envoi.** `terminal.send` avec `if: idle`, confirmation par `turn` /
  `started`, refus `blocked` affiché tel quel ; idempotence côté serveur.
- **P6 — Questions, arrêt, touches, pièces jointes** (`interactions`,
  `interrupt`, `keys`, `put`).
- **P7 — Lancement d'agents** : `terminal.run` avec labels `herdr.space` /
  `herdr.tab` pour apparaître dans l'app (à vérifier), `no_parent`.
- **P8 — Packaging** (seulement sur instruction explicite).
- Plus tard : autres machines (`machine/t7`), vue terminal live
  (`terminal.attach`), « vu »/« épinglé » partagés.

## 7. Questions ouvertes

- Copie du code ou socle partagé avec Herdr Chat ?
- Version minimale herdr-core supportée et porte de compatibilité (0.1.x
  publie tous les quelques jours).
- Pi sous herdr-core : émet-il l'OSC 7501 ? Comment retrouver sa session ?
- Ports, noms et identité de la PWA (§5).
