#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Turns a Linux package folder holding the binaries `suhina`, `suhina-cli` and
# `suhina-plugin-host` into the two files a Linux user installs YOBUKO from:
#
#   YOBUKO-<version>-x86_64.AppImage  one file that runs on most distributions: make it
#                                      executable and double-click it, nothing is installed.
#   yobuko_<version>_amd64.deb         for Debian, Ubuntu, Mint, Pop!_OS…: installs the app in
#                                      /opt/yobuko, a `yobuko` command and a menu entry.
#
# In both, the three binaries stay side by side so the app still finds the plug-in host next to
# itself. The binaries link only to system libraries; ALSA (libasound) is the one a user may have
# to install, and the window opens through X11 or Wayland with OpenGL, which every desktop has.
#
# The AppImage needs appimagetool: set APPIMAGETOOL to its path, or it is downloaded. Build on the
# oldest distribution to support (CI uses Ubuntu 22.04): the binaries need its glibc or newer.
#
# Usage: scripts/linux_package.sh <package-folder> <version> <output-folder>
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <package-folder> <version> <output-folder>" >&2
  exit 2
fi
package=$(cd "$1" && pwd)
version=$2
mkdir -p "$3"
out=$(cd "$3" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
icon="$root/assets/brand/suhina-icon.png"
# Debian versions sort `~` before anything, so a nightly comes before the release it leads to,
# and take no `+` build metadata in the upstream part the way semver does.
deb_version=$(printf '%s' "$version" | sed -e 's/-/~/' -e 's/+/~g/')

for bin in suhina suhina-cli suhina-plugin-host; do
  if [ ! -x "$package/$bin" ]; then
    echo "missing executable: $package/$bin" >&2
    exit 1
  fi
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

desktop_entry() {
  # $1: the command the menu entry runs.
  cat <<EOF
[Desktop Entry]
Type=Application
Name=YOBUKO
GenericName=Digital audio workstation
Comment=Make music, alone or together over the Internet
Exec=$1 %F
Icon=yobuko
Terminal=false
Categories=AudioVideo;Audio;Midi;Music;
Keywords=DAW;music;audio;MIDI;multiplayer;
StartupWMClass=Suhina
EOF
}

install_icons() {
  # $1: the share folder the hicolor icons go into.
  for size in 48 128 256 512; do
    mkdir -p "$1/icons/hicolor/${size}x${size}/apps"
    convert "$icon" -resize "${size}x${size}" "$1/icons/hicolor/${size}x${size}/apps/yobuko.png"
  done
}

copy_programs() {
  # $1: the folder the three programs, the licences and the distribution metadata go into.
  mkdir -p "$1"
  cp -a "$package"/. "$1"/
  chmod 0755 "$1/suhina" "$1/suhina-cli" "$1/suhina-plugin-host"
}

# --- .deb -------------------------------------------------------------------------------------
deb="$work/deb"
copy_programs "$deb/opt/yobuko"
mkdir -p "$deb/usr/bin" "$deb/usr/share/applications" "$deb/DEBIAN"
ln -s /opt/yobuko/suhina "$deb/usr/bin/yobuko"
ln -s /opt/yobuko/suhina-cli "$deb/usr/bin/yobuko-cli"
desktop_entry /opt/yobuko/suhina > "$deb/usr/share/applications/yobuko.desktop"
install_icons "$deb/usr/share"
mkdir -p "$deb/usr/share/doc/yobuko"
cp "$root/LICENSE" "$deb/usr/share/doc/yobuko/copyright"
size=$(du -sk "$deb" | cut -f1)
cat > "$deb/DEBIAN/control" <<EOF
Package: yobuko
Version: $deb_version
Architecture: amd64
Maintainer: YOBUKO contributors <noreply@github.com>
Installed-Size: $size
Depends: libasound2t64 | libasound2, libc6 (>= 2.35), libgl1, libxkbcommon0
Recommends: libx11-6, libxcursor1, libxi6, libxrandr2, libxkbcommon-x11-0, libwayland-client0, xdg-desktop-portal
Section: sound
Priority: optional
Homepage: https://github.com/Nozomagarion/YOBUKO
Description: YOBUKO, a music workstation you can share over the Internet
 A fork of the Suhina digital audio workstation: tracks, clips, MIDI, built-in
 instruments and effects, CLAP/VST3/VST2 plug-ins, and online sessions where
 friends join with a short code. Free software under GPL-3.0-or-later.
EOF
dpkg-deb --root-owner-group --build "$deb" "$out/yobuko_${deb_version}_amd64.deb"

# --- AppImage ---------------------------------------------------------------------------------
app="$work/YOBUKO.AppDir"
copy_programs "$app/usr/bin"
install_icons "$app/usr/share"
mkdir -p "$app/usr/share/applications"
desktop_entry suhina > "$app/usr/share/applications/yobuko.desktop"
cp "$app/usr/share/applications/yobuko.desktop" "$app/yobuko.desktop"
cp "$app/usr/share/icons/hicolor/256x256/apps/yobuko.png" "$app/yobuko.png"
cat > "$app/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/suhina" "$@"
EOF
chmod 0755 "$app/AppRun"

tool=${APPIMAGETOOL:-}
if [ -z "$tool" ]; then
  tool="$work/appimagetool"
  curl -fsSL -o "$tool" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod 0755 "$tool"
fi
# CI machines have no FUSE: appimagetool unpacks itself instead of mounting.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$tool" --no-appstream "$app" \
  "$out/YOBUKO-$version-x86_64.AppImage"
chmod 0755 "$out/YOBUKO-$version-x86_64.AppImage"

ls -l "$out"
