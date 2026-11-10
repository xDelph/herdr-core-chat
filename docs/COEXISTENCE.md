# Coexistence avec Herdr Chat

> Remplace `BASCULE.md` (copié d'Herdr Chat : procédure de bascule TS → Rust
> d'Herdr Chat, sans objet ici). herdr-core-chat **ne remplace pas** Herdr
> Chat : les deux tournent ensemble pendant la migration (AGENTS §0, §5).

## Ce qui diffère (vérifié par les tests)

| | Herdr Chat (`../herdr-pwa`) | herdr-core-chat |
|---|---|---|
| Amont | Herdr 0.9.3, `~/.config/herdr/herdr.sock` | herdr-core, `~/.local/state/herdr-core/default/default.sock` |
| Binaire | `herdrchat` | `herdr-core-chat` |
| Port local | `8791` | `8792` |
| Tailscale Serve | `:8790` (et `:8443`) → `8791` | `:443` → `8792` |
| Variables | `HERDR_REMOTE_*`, `HERDR_SOCKET_PATH` | `HERDR_CORE_CHAT_*` — les noms d'Herdr Chat ne sont **jamais lus** (`tests/unit/config.rs`) |
| Données | `~/.herdr-remote/` (`push.json`, `reads.json`, journal) | `~/.herdr-core-chat/` — le store d'Herdr Chat n'est **jamais importé** (`tests/unit/store_push.rs`) |
| Clés VAPID | celles de `~/.herdr-remote/push.json` | générées au premier lancement, à part |
| Sujet VAPID par défaut | `mailto:herdr-remote@example.com` | `mailto:herdr-core-chat@example.com` |
| PWA | « Herdr Chat », `id: "/"`, icône bleue | « Herdr Core Chat », `id: "/herdr-core-chat"`, icône ambre |
| Stockage navigateur | `herdr-chat.*`, cache `herdr-navigation` | `herdr-core-chat.*`, cache `herdr-core-chat-navigation` |
| Tap / formule | `xDelph/herdr-chat` → `herdr-chat` | le **même** tap → `herdr-core-chat` (`brew install xDelph/herdr-chat/herdr-core-chat`) |
| Service launchd | `sh.brew.herdr-chat` | `sh.brew.herdr-core-chat` |
| Journal du service | `$(brew --prefix)/var/log/herdr-chat.log` | `$(brew --prefix)/var/log/herdr-core-chat.log` |

## Points d'attention

- **Même origine, deux applications.** Si une origine Tailscale a servi Herdr
  Chat puis sert herdr-core-chat (relevé du 2026-11-03 : `:443` → `8792`), la
  PWA installée depuis cette origine est l'ancienne : le navigateur garde son
  service worker et son abonnement push jusqu'à ce qu'elle se recharge. Le
  nouvel `id` de manifeste et les clés de stockage distinctes évitent de
  confondre les deux, mais l'abonnement push d'Herdr Chat reste lié à ses
  propres clés : réinstaller la PWA depuis l'origine de herdr-core-chat et y
  réactiver les notifications.
- **Icône** : la même invite qu'Herdr Chat, en **ambre** (Herdr Chat : bleu), avec
  un « cœur » à la place du curseur — sources `apps/web/icones/*.svg`,
  `scripts/icones.sh` puis `scripts/ecrans-demarrage.sh`.
- **Un port, un démon.** Le démon prend son port avant toute autre chose : une
  seconde instance sur `8792` (service Homebrew + démon lancé depuis le dépôt)
  s'arrête sur `Address already in use` sans avoir lu Herdr ni composé ses
  notifications. Les origines Tailscale autorisées sont celles qui proxifient
  vers **son** port exact : `:8790`/`:8443` (Herdr Chat) ne lui ouvrent rien.
- Bascule du démon de dev vers le service Homebrew, contrôles et retour
  arrière : [`PACKAGING.md`](PACKAGING.md) §3.
- Aucun service, Homebrew, Tailscale ni launchd n'est touché par ce dépôt sans
  instruction explicite (AGENTS §1.9).
