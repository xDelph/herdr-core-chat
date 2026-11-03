#!/usr/bin/env bash
# Icônes de la PWA — générées depuis apps/web/icones/*.svg, pas dessinées à
# la main. Distinctes de celles d'Herdr Chat (AGENTS §5 : nom/icône propres).
#
# ⚠️ Le rendu SVG d'ImageMagick ignore les tracés en `stroke` : le 512 px est
# rendu par Chrome sans tête (`CHROME`, défaut : Google Chrome de macOS),
# ImageMagick ne fait que réduire.
#
# Usage : scripts/icones.sh, puis scripts/ecrans-demarrage.sh (les écrans de
# démarrage reprennent icon-maskable-512.png).
set -euo pipefail
command -v magick >/dev/null || { echo "ImageMagick (magick) est requis" >&2; exit 1; }
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[ -x "$chrome" ] || { echo "Chrome introuvable : $chrome (variable CHROME)" >&2; exit 1; }
racine="$(cd "$(dirname "$0")/.." && pwd)"
source="$racine/apps/web/icones"
public="$racine/apps/web/public"
temporaire="$(mktemp -d)"
trap 'rm -rf "$temporaire"' EXIT
rend512() {
  "$chrome" --headless --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
    --window-size=512,512 --screenshot="$2" "file://$1" >/dev/null 2>&1
  [ -s "$2" ] || { echo "rendu Chrome vide : $1" >&2; exit 1; }
}
rend512 "$source/icone.svg" "$temporaire/icone.png"
rend512 "$source/icone-maskable.svg" "$temporaire/masquable.png"
reduit() { magick "$1" -resize "$2x$2" -strip "PNG32:$3"; }
reduit "$temporaire/icone.png" 512 "$public/icon-512.png"
reduit "$temporaire/icone.png" 192 "$public/icon-192.png"
reduit "$temporaire/icone.png" 180 "$public/apple-touch-icon.png"
reduit "$temporaire/icone.png" 32 "$public/favicon.png"
reduit "$temporaire/masquable.png" 512 "$public/icon-maskable-512.png"
reduit "$temporaire/masquable.png" 192 "$public/icon-maskable-192.png"
echo "icônes écrites dans $public"
