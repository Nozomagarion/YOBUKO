#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Turns a macOS package folder that holds `Suhina.app` (made by scripts/macos_app.sh) into the
# one file a Mac user installs YOBUKO from: a disk image that opens on the app, a link to
# Applications to drag it onto, and a short note on opening an app from an unidentified
# developer the first time (the app is signed ad hoc, not notarised).
#
# The bundle is copied as `YOBUKO.app`: a bundle's signature covers its Contents, not its folder
# name, so the ad hoc signature stays valid.
#
# Usage: scripts/macos_dmg.sh <package-folder> <output.dmg>
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <package-folder> <output.dmg>" >&2
  exit 2
fi
package=$1
output=$2
if [ ! -d "$package/Suhina.app" ]; then
  echo "missing app bundle: $package/Suhina.app" >&2
  exit 1
fi

stage=$(mktemp -d)/YOBUKO
mkdir -p "$stage"
cp -R "$package/Suhina.app" "$stage/YOBUKO.app"
ln -s /Applications "$stage/Applications"
cat > "$stage/LISEZ-MOI - READ ME.txt" <<'EOF'
YOBUKO

FR
1. Glisse YOBUKO.app sur le dossier Applications.
2. La premiere fois : clic droit (ou Ctrl + clic) sur YOBUKO dans Applications, puis Ouvrir,
   puis Ouvrir encore. macOS previent car l'app ne vient pas de l'App Store ; ensuite elle
   s'ouvre normalement.
   Si macOS dit seulement qu'il ne peut pas l'ouvrir : Reglages Systeme > Confidentialite et
   securite, tout en bas, bouton "Ouvrir quand meme".

EN
1. Drag YOBUKO.app onto the Applications folder.
2. The first time: right-click (or Ctrl-click) YOBUKO in Applications, choose Open, then Open
   again. macOS warns because the app is not from the App Store; after that it opens normally.
   If macOS only says it cannot open it: System Settings > Privacy & Security, at the bottom,
   "Open Anyway".
EOF

mkdir -p "$(dirname "$output")"
rm -f "$output"
hdiutil create -volname YOBUKO -srcfolder "$stage" -ov -format UDZO "$output"
hdiutil verify "$output"
