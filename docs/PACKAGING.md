# herdr-core-chat — packaging Homebrew

> État au 2026-11-11 : **publié et basculé.** Dépôt public `xDelph/herdr-core-chat`
> (historique anonymisé avant le premier push), releases `v0.1.0` puis `v0.1.1`
> (CI `release-rust` verte, 4 cibles), formule dans le tap `xDelph/homebrew-herdr-chat`.
> Sur ce Mac, le service `herdr-core-chat` 0.1.1 tient `127.0.0.1:8792` ; le démon
> de dev est arrêté (dev désormais sur 8793, voir §3). Une nouvelle version suit
> §2, puis `brew upgrade` — sur instruction explicite (AGENTS §1.9). L'historique
> d'Herdr Chat est dans `../herdr-pwa/docs/PACKAGING.md`.

## Les noms

| | Herdr Chat (inchangé) | Herdr Core Chat |
|---|---|---|
| Dépôt du code | `xDelph/herdr-chat` | `xDelph/herdr-core-chat` |
| Tap | `xDelph/homebrew-herdr-chat` | **le même** : `Formula/herdr-core-chat.rb` à côté de `herdr-chat.rb` |
| Formule | `herdr-chat` | `herdr-core-chat` (classe `HerdrCoreChat`) |
| Installation | `brew install xDelph/herdr-chat` | `brew install xDelph/herdr-chat/herdr-core-chat` |
| Binaire | `herdrchat` | `herdr-core-chat` |
| Service launchd | `sh.brew.herdr-chat` | `sh.brew.herdr-core-chat` |
| Journal du service | `$(brew --prefix)/var/log/herdr-chat.log` | `$(brew --prefix)/var/log/herdr-core-chat.log` |

Un tap, deux formules, deux versions indépendantes. Le raccourci
`xDelph/herdr-chat` ne vaut que pour la formule du même nom ; l'autre se nomme
en entier (`xDelph/herdr-chat/herdr-core-chat`). Après installation, les
commandes courtes marchent (`brew services start herdr-core-chat`,
`brew upgrade herdr-core-chat`). Le tap est déjà approuvé sur ce Mac
(`brew trust` : `xdelph/herdr-chat`).

## Ce qui est prêt (et vérifié le 2026-11-08)

| Pièce | Où | Preuve |
|---|---|---|
| Version | `0.1.0` (workspace Cargo, `package.json`, `--version`) | `tests/unit/version.rs` ; `herdr-core-chat 0.1.0` |
| Archive | `cargo run --locked -p xtask -- release` → `dist/herdr-core-chat-<cible>.tar.gz` + `.sha256` | archive macOS arm64 construite et exécutée : PWA embarquée, aucun OpenSSL système, sert `/` et `/api/health` avec `PATH=/usr/bin:/bin` |
| CI de release | `.github/workflows/release-rust.yml` (étiquette `v*`) : portes → 4 binaires natifs → release GitHub + `SHA256SUMS` | portes rejouées en local : `xtask verify`, `xtask coverage`, `bun run verify` (752 tests) — vertes. Le workflow lui-même n'a **jamais tourné** (pas de dépôt) |
| Formule | gabarit `packaging/herdr-core-chat.rb` | `brew style` et `brew audit --strict` : aucune remarque |
| Remplissage | `scripts/formule-release.sh <version>` : empreintes du `SHA256SUMS` de la release, version dans les URL, écrit dans le clone Homebrew du tap (`$(brew --repository xdelph/herdr-chat)/Formula/`) | refuse une version ≠ workspace, une cible sans empreinte, une empreinte invalide |
| Essai d'installation | `scripts/essai-formule-locale.sh` : tap jetable, archive locale, `brew install` + `brew test`, plist du service **lu** (label, port, bind), binaire installé lancé sur un port d'essai, puis tout est retiré | passé sur ce Mac ; après coup : aucune formule, aucun tap, aucune confiance résiduels, Herdr Chat et le démon de dev intacts |
| Service macOS | bloc `service` de la formule (`brew services`) | plist généré : `sh.brew.herdr-core-chat`, `127.0.0.1:8792`, `PATH` Homebrew + `~/.local/bin` + `~/.bun/bin` + système |
| Service Linux | `packaging/herdr-core-chat.service` (systemd utilisateur), installé dans `pkgshare` | non essayé |

## 1. Publier (une fois)

1. Créer le dépôt **public** `xDelph/herdr-core-chat` (public : Homebrew télécharge
   les archives de release sans jeton, comme pour Herdr Chat), y pousser
   `develop`, et une branche `main` au même point.
2. Rien à créer pour le tap : la formule rejoint `xDelph/homebrew-herdr-chat`
   (§2). Ajouter au README du tap une section `herdr-core-chat` (installation
   ci-dessous, « runs alongside Herdr Chat, port 8792 »).

## 2. Livrer une version

```bash
# Version déjà à 0.1.0 pour la première. Sinon : Cargo.toml + package.json,
# puis `cargo update -w` (Cargo.lock), commit.
git tag v0.1.0 && git push origin v0.1.0          # → release-rust.yml
gh run watch -R xDelph/herdr-core-chat             # portes, 4 binaires, release
scripts/formule-release.sh 0.1.0                   # → $(brew --repository xdelph/herdr-chat)/Formula/herdr-core-chat.rb
cd "$(brew --repository xdelph/herdr-chat)" && git add Formula/herdr-core-chat.rb README.md \
  && git commit -m "herdr-core-chat 0.1.0" && git push
```

`macos-15-intel` est le dernier runner Intel hébergé : s'il manque, la cible
x86_64 passe en compile-only (voir l'en-tête du workflow), jamais « validée »
sans exécution.

## 3. Basculer ce Mac : démon de dev → service Homebrew

Aujourd'hui `127.0.0.1:8792` est tenu par le démon lancé depuis le dépôt
(`target/release/herdr-core-chat`, `nohup`, PPID 1). Le service Homebrew prend
**le même port, le même dossier de données et la même entrée Tailscale** : la
PWA installée sur le téléphone (origine `https://<machine>.<tailnet>.ts.net`,
`:443`) continue de marcher, avec les mêmes clés VAPID et les mêmes abonnements
push — rien à réinstaller, rien à réabonner.

```bash
# a. Arrêter le démon de dev — après l'avoir identifié (chemin, PPID 1, port 8792).
pgrep -fl "$PWD/target/release/herdr-core-chat"
lsof -nP -iTCP:8792 -sTCP:LISTEN
kill <pid>

# b. Installer et démarrer.
brew update                                        # récupère la nouvelle formule du tap
brew install xDelph/herdr-chat/herdr-core-chat     # tap déjà approuvé ici
brew services start herdr-core-chat

# c. Contrôles.
brew services list | grep herdr                    # herdr-chat ET herdr-core-chat : started
lsof -nP -iTCP -sTCP:LISTEN | grep -E ':879[12]'   # 8791 herdrchat, 8792 /opt/homebrew/…/herdr-core-chat
curl -s http://127.0.0.1:8792/api/health           # {"ok":true,…,"herdr":{"reachable":true…}}
curl -s http://127.0.0.1:8791/api/health           # Herdr Chat, inchangé
tailscale serve status                             # :443 → 8792 ; :8790 et :8443 → 8791 (inchangés)
tail -n 20 "$(brew --prefix)/var/log/herdr-core-chat.log"   # « origines Tailscale Serve autorisées » avec l'origine :443
```

Puis sur le téléphone : ouvrir Herdr Core Chat (liste des agents, une
conversation, un envoi), et Herdr Chat à côté — chacun voit son amont.

⚠️ **Ne pas sauter l'étape a.** Avec le démon de dev encore là, le service
échoue sur `Address already in use` et `KeepAlive` le relance en boucle (visible
dans le journal du service). Depuis le 2026-11-08, le démon prend son port
**avant** de lire Herdr ou de composer ses notifications : une seconde instance
s'arrête sans avoir rien lu ni rien envoyé (vérifié : sortie 1, aucun dossier
de données créé).

### Retour arrière

```bash
brew services stop herdr-core-chat
nohup bin/herdr-core-chat >> ~/.herdr-core-chat/daemon.log 2>&1 < /dev/null & disown
```

Les données (`~/.herdr-core-chat/`) sont les mêmes des deux côtés.

### Après la bascule : le développement

Le port `8792` et `~/.herdr-core-chat/` appartiennent alors au service. Un démon
lancé depuis le dépôt pour essayer un changement doit prendre **un autre port
ET un autre magasin**, sinon il partagerait les abonnements push (notifications
en double) :

```bash
HERDR_CORE_CHAT_PORT=8793 \
HERDR_CORE_CHAT_PUSH_STORE="$HOME/.herdr-core-chat-dev/push.json" \
HERDR_CORE_CHAT_LOG_PATH="$HOME/.herdr-core-chat-dev/herdr-core-chat.log" \
  bin/herdr-core-chat
```

Livrer un changement au téléphone = une release puis
`brew upgrade herdr-core-chat && brew services restart herdr-core-chat` (la PWA
servie vient du binaire installé, pas du dépôt).

## Piège connu (hérité, toujours vrai)

Sous launchd (`/usr/bin:/bin`), le démon lance `git`, `tailscale` et les CLI
des agents par leur nom : sans `PATH`, un agent installé hors de Homebrew n'a
plus de catalogue. Le bloc `service` donne donc un `PATH` (Homebrew,
`~/.local/bin`, `~/.bun/bin`, système), et la formule le dit dans ses
`caveats`. Sur ce Mac : `hc`, `herdr-app`, `claude` dans `~/.local/bin`, `pi`
dans `~/.bun/bin`, `codex`/`opencode`/`devin` dans `/opt/homebrew/bin`,
`tailscale` dans `/usr/local/bin` — tous couverts.

## Encore ouvert

- Version minimale de herdr-core supportée : seule la génération de protocole
  `1` (herdr-core 0.1.17) est vérifiée ; une autre génération est refusée.
- Linux : archives construites par la CI, service systemd fourni, jamais essayé
  contre un herdr-core Linux.
