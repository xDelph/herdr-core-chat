#!/usr/bin/env bash
#
# Essai de la formule Homebrew AVANT publication, sur cette machine, sans
# GitHub et sans rien démarrer.
#
#   cargo run --locked -p xtask -- release      # produit dist/<archive>
#   scripts/essai-formule-locale.sh [--garder]
#
# Ce qu'il fait :
#   1. un tap jetable `xdelph/herdr-core-chat-essai` (local, sans git) ;
#   2. la formule du gabarit, remplie par `formule-release.sh` avec l'archive
#      LOCALE de dist/ (URL file://, empreinte du .sha256 local) ;
#   3. `brew install` puis `brew test` (le `test do` de la formule) ;
#   4. le plist que `brew services` écrirait — lu, jamais chargé — et ses
#      invariants de coexistence (label, port 8792, bind loopback) ;
#   5. le binaire INSTALLÉ lancé sur un port d'essai avec un HOME jetable :
#      /api/health répond (503 : pas de herdr-core dans ce HOME) et la PWA aussi ;
#   6. désinstallation et retrait du tap (sauf --garder).
#
# Ne touche ni au service `herdr-chat`, ni au démon de dev, ni au tap publié.
# Refuse de tourner si une formule `herdr-core-chat` est déjà installée.
set -euo pipefail

ici="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
racine="$(cd "${ici}/.." && pwd)"
tap="xdelph/herdr-core-chat-essai"
formule="${tap}/herdr-core-chat"
port_essai="${HERDR_CORE_CHAT_PORT_ESSAI:-8799}"
garder=0
[[ "${1:-}" == "--garder" ]] && garder=1

export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_FROM_API=1 HOMEBREW_NO_ENV_HINTS=1

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) etiquette=macos-arm64 ;;
  Darwin-x86_64) etiquette=macos-x86_64 ;;
  Linux-x86_64) etiquette=linux-x86_64 ;;
  Linux-aarch64) etiquette=linux-aarch64 ;;
  *) echo "essai : machine non reconnue" >&2; exit 1 ;;
esac
archive="${racine}/dist/herdr-core-chat-${etiquette}.tar.gz"
if [[ ! -f "${archive}" || ! -f "${archive}.sha256" ]]; then
  echo "essai : ${archive} absente — lancer d'abord : cargo run --locked -p xtask -- release" >&2
  exit 1
fi
if brew list --formula herdr-core-chat > /dev/null 2>&1; then
  echo "essai : une formule herdr-core-chat est déjà installée — rien n'est touché." >&2
  exit 1
fi

travail="$(mktemp -d)"
nettoie() {
  if [[ -n "${pid_essai:-}" ]]; then kill "${pid_essai}" 2> /dev/null || true; fi
  if [[ ${garder} -eq 0 ]]; then
    brew uninstall --formula "${formule}" > /dev/null 2>&1 || true
    brew untrust --tap "${tap}" > /dev/null 2>&1 || true
    brew untrust --formula "${formule}" > /dev/null 2>&1 || true
    brew untap "${tap}" > /dev/null 2>&1 || true
  fi
  rm -rf "${travail}"
}
trap nettoie EXIT

echo "== 1. tap jetable ${tap}"
brew tap-new --no-git "${tap}" > /dev/null
brew trust --tap "${tap}" > /dev/null
dossier_tap="$(brew --repository "${tap}")"

echo "== 2. formule depuis le gabarit (archive locale)"
HERDR_CORE_CHAT_SHA256SUMS="${archive}.sha256" \
  HERDR_CORE_CHAT_URL_BASE="file://${racine}/dist" \
  "${ici}/formule-release.sh" "$(sed -n 's/^version = "\(.*\)"$/\1/p' "${racine}/Cargo.toml" | head -1)" \
  "${dossier_tap}/Formula/herdr-core-chat.rb" --partiel

echo "== 3. brew install + brew test"
brew install --formula "${formule}"
brew test "${formule}"
binaire="$(brew --prefix)/opt/herdr-core-chat/bin/herdr-core-chat"
"${binaire}" --version

echo "== 4. le plist de brew services (lu, non chargé)"
brew ruby -e "puts Formula['${formule}'].service.to_plist" > "${travail}/service.plist"
plutil -lint "${travail}/service.plist" 2> /dev/null || true
lit() { /usr/libexec/PlistBuddy -c "Print :$1" "${travail}/service.plist"; }
echoue() { echo "ÉCHEC : $*" >&2; exit 1; }
[[ "$(lit Label)" == "sh.brew.herdr-core-chat" ]] || echoue "label $(lit Label)"
[[ "$(lit EnvironmentVariables:HERDR_CORE_CHAT_PORT)" == "8792" ]] || echoue "port"
[[ "$(lit EnvironmentVariables:HERDR_CORE_CHAT_BIND)" == "127.0.0.1" ]] || echoue "bind"
[[ "$(lit ProgramArguments:0)" == "${binaire}" ]] || echoue "programme $(lit ProgramArguments:0)"
if grep -q HERDR_REMOTE "${travail}/service.plist"; then echoue "variable d'Herdr Chat dans le plist"; fi
echo "label=$(lit Label) port=8792 bind=127.0.0.1 journal=$(lit StandardOutPath)"
echo "PATH=$(lit EnvironmentVariables:PATH)"

echo "== 5. le binaire installé sert (port ${port_essai}, HOME jetable)"
if lsof -nP -iTCP:"${port_essai}" -sTCP:LISTEN > /dev/null 2>&1; then
  echoue "port d'essai ${port_essai} déjà occupé"
fi
mkdir -p "${travail}/home"
env -i PATH=/usr/bin:/bin HOME="${travail}/home" HERDR_CORE_CHAT_PORT="${port_essai}" \
  "${binaire}" > "${travail}/essai.log" 2>&1 &
pid_essai=$!
sante=000
for _ in $(seq 1 40); do
  sante="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:${port_essai}/api/health" || true)"
  [[ "${sante}" != "000" ]] && break
  sleep 0.25
done
page="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:${port_essai}/")"
echo "sante=${sante} pwa=${page}"
[[ "${sante}" == "503" || "${sante}" == "200" ]] || { cat "${travail}/essai.log"; echoue "santé ${sante}"; }
[[ "${page}" == "200" ]] || echoue "PWA ${page}"

echo "== OK — formule installable, testée, service conforme."
if [[ ${garder} -eq 1 ]]; then
  echo "(--garder : ${formule} reste installée ; retirer avec : brew uninstall ${formule} && brew untap ${tap})"
fi
