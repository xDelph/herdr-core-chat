# herdr-core-chat — packaging (préparé, non publié)

> ⛔ P8 : rien n'est publié, installé ni démarré par ce dépôt sans instruction
> explicite (AGENTS §1.9). L'historique du packaging d'Herdr Chat (forme A
> TS, tap `xDelph/homebrew-herdr-chat`) vit dans `../herdr-pwa/docs/PACKAGING.md`.

## Ce qui est prêt

| Sujet | herdr-core-chat |
|---|---|
| Binaire | `herdr-core-chat` (paquet Cargo `herdr_core_chat`), PWA **embarquée** par `crates/herdr_core_chat/build.rs` ; un build de release sans `apps/web/dist` échoue (`HERDR_CORE_CHAT_REQUIRE_PWA=1` le prouve hors release) |
| Archive | `xtask release` → `dist/herdr-core-chat-<cible>.tar.gz` : `herdr-core-chat`, `LICENSE`, `README.md`, `manifest.json`, gabarits de service ; `.sha256` à côté |
| Cibles CI | `.github/workflows/release-rust.yml` : macOS arm64/x86_64, Linux x86_64/aarch64 — jamais exécuté |
| Formule | gabarit `packaging/herdr-core-chat.rb` (classe `HerdrCoreChat`) — tap et dépôt **à confirmer**, URL/empreintes à remplir depuis une release |
| Service macOS | `brew services` (bloc `service` de la formule : `PATH` pour `git`, `tailscale` et les CLI d'agents ; journal `$(brew --prefix)/var/log/herdr-core-chat.log`) ; LaunchAgent de secours `packaging/dev.herdr.core-chat.plist` |
| Service Linux | `packaging/herdr-core-chat.service` (systemd utilisateur) |

## Coexistence

Tout diffère d'Herdr Chat (voir [`COEXISTENCE.md`](COEXISTENCE.md)) : formule
et service `herdr-core-chat`, port `8792`, données `~/.herdr-core-chat/`,
variables `HERDR_CORE_CHAT_*`. Les deux services peuvent tourner ensemble.

## Piège connu (hérité, toujours vrai)

Sous launchd (`/usr/bin:/bin`), le démon lance `git`, `tailscale` et les CLI
des agents par leur nom : sans `PATH`, un agent installé hors de Homebrew n'a
plus de catalogue. Le bloc `service` donne donc un `PATH` (Homebrew,
`~/.local/bin`, `~/.bun/bin`, système), et la formule le dit dans ses
`caveats`.

## À décider avant publication

- Dépôt et tap (`xDelph/herdr-core-chat` ? même tap qu'Herdr Chat ?).
- Nom court de la PWA (« Core Chat ») et icône ambre : à valider.
- Version minimale de herdr-core supportée (seule la génération de protocole
  `1`, herdr-core 0.1.17, est vérifiée ; une autre génération est refusée).
