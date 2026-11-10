#!/usr/bin/env bash
#
# Écrit la formule Homebrew PUBLIÉE à partir du gabarit
# `packaging/herdr-core-chat.rb` et des empreintes d'une release.
#
#   scripts/formule-release.sh <version> [destination]
#
# Destination par défaut : le tap partagé avec Herdr Chat (son clone Homebrew),
#   $(brew --repository xdelph/herdr-chat)/Formula/herdr-core-chat.rb
#
# Les empreintes viennent du `SHA256SUMS` de la release GitHub `v<version>`
# (`gh release download`), jamais d'un build local : les archives de la CI ne
# sont pas celles de `dist/` (un build n'est pas reproductible octet pour
# octet).
#
# Essai local AVANT publication (scripts/essai-formule-locale.sh) :
#   HERDR_CORE_CHAT_SHA256SUMS=<fichier>   empreintes lues dans ce fichier
#   HERDR_CORE_CHAT_URL_BASE=file:///…/dist   archives lues là, pas sur GitHub
#   --partiel                               cibles absentes tolérées (marqueur gardé)
#
# Refuse : une version différente de celle du workspace Cargo, une cible
# sans empreinte (hors --partiel), une empreinte qui n'a pas 64 hexadécimaux,
# une formule dont Ruby rejette la syntaxe.
set -euo pipefail

ici="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
racine="$(cd "${ici}/.." && pwd)"
gabarit="${racine}/packaging/herdr-core-chat.rb"
depot="xDelph/herdr-core-chat"
etiquettes=(macos-arm64 macos-x86_64 linux-x86_64 linux-aarch64)

partiel=0
arguments=()
for argument in "$@"; do
  case "${argument}" in
    --partiel) partiel=1 ;;
    *) arguments+=("${argument}") ;;
  esac
done
if [[ ${#arguments[@]} -lt 1 ]]; then
  echo "usage : scripts/formule-release.sh <version> [destination] [--partiel]" >&2
  exit 2
fi
version="${arguments[0]#v}"
if [[ -n "${arguments[1]:-}" ]]; then
  destination="${arguments[1]}"
else
  destination="$(brew --repository xdelph/herdr-chat)/Formula/herdr-core-chat.rb"
  if [[ ! -d "$(dirname "${destination}")" ]]; then
    echo "formule : tap xdelph/herdr-chat absent — brew tap xDelph/herdr-chat, ou passer une destination." >&2
    exit 1
  fi
fi

# La version publiée est celle du binaire : `--version` doit dire la même.
version_workspace="$(sed -n 's/^version = "\(.*\)"$/\1/p' "${racine}/Cargo.toml" | head -1)"
if [[ "${version}" != "${version_workspace}" ]]; then
  echo "formule : version ${version} ≠ workspace ${version_workspace} (Cargo.toml)." >&2
  exit 1
fi

travail="$(mktemp -d)"
trap 'rm -rf "${travail}"' EXIT
if [[ -n "${HERDR_CORE_CHAT_SHA256SUMS:-}" ]]; then
  cp "${HERDR_CORE_CHAT_SHA256SUMS}" "${travail}/SHA256SUMS"
else
  gh release download "v${version}" -R "${depot}" -p SHA256SUMS -D "${travail}"
fi

# `<empreinte>  herdr-core-chat-<etiquette>.tar.gz` → une variable par cible.
empreintes=""
for etiquette in "${etiquettes[@]}"; do
  empreinte="$(awk -v f="herdr-core-chat-${etiquette}.tar.gz" '$2 == f || $2 == "*" f { print $1 }' "${travail}/SHA256SUMS" | head -1)"
  if [[ -z "${empreinte}" ]]; then
    if [[ ${partiel} -eq 1 ]]; then
      continue
    fi
    echo "formule : aucune empreinte pour ${etiquette} dans SHA256SUMS." >&2
    exit 1
  fi
  if ! [[ "${empreinte}" =~ ^[0-9a-f]{64}$ ]]; then
    echo "formule : empreinte invalide pour ${etiquette} : « ${empreinte} »." >&2
    exit 1
  fi
  empreintes+="herdr-core-chat-${etiquette}.tar.gz=${empreinte};"
done

prefixe_github="https://github.com/${depot}/releases/download/v"
base="${HERDR_CORE_CHAT_URL_BASE:-}"

# Le gabarit : la version dans chaque URL GitHub (Homebrew la lit là), ou la
# base locale ET un `version` explicite (une URL file:// n'en porte pas), puis
# l'empreinte sous chaque `url`.
awk -v version="${version}" -v prefixe="${prefixe_github}" -v base="${base}" -v table="${empreintes}" '
  BEGIN {
    n = split(table, lignes, ";")
    for (i = 1; i <= n; i++) {
      if (lignes[i] == "") continue
      egal = index(lignes[i], "=")
      sha[substr(lignes[i], 1, egal - 1)] = substr(lignes[i], egal + 1)
    }
  }
  /^  license "/ && base != "" { print; print "  version \"" version "\""; next }
  /url "/ {
    archive = $0
    sub(/^.*\//, "", archive); sub(/".*$/, "", archive)
    i = index($0, prefixe)
    if (i > 0) {
      reste = substr($0, i + length(prefixe))
      reste = substr(reste, index(reste, "/"))
      cible = (base != "") ? base : prefixe version
      $0 = substr($0, 1, i - 1) cible reste
    }
    print; next
  }
  /sha256 "A_REMPLACER_PAR_XTASK_RELEASE"/ && (archive in sha) {
    sub(/A_REMPLACER_PAR_XTASK_RELEASE/, sha[archive])
  }
  { print }
' "${gabarit}" > "${travail}/herdr-core-chat.rb"

ruby -c "${travail}/herdr-core-chat.rb" > /dev/null
if [[ ${partiel} -eq 0 ]] && grep -q A_REMPLACER_PAR_XTASK_RELEASE "${travail}/herdr-core-chat.rb"; then
  echo "formule : un marqueur d'empreinte est resté." >&2
  exit 1
fi

mkdir -p "$(dirname "${destination}")"
cp "${travail}/herdr-core-chat.rb" "${destination}"
echo "formule ${version} écrite : ${destination}"
