#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Puts Mixxx (free DJ software by the Mixxx team, GPL-2.0-or-later, mixxx.org) where YOBUKO
# looks for it, so DJ mode opens it (crates/ui/src/app/dj_mixxx.rs › bundled). The official
# release is downloaded unchanged from downloads.mixxx.org:
#
#   macOS:    <package>/Suhina.app/Contents/Helpers/Mixxx.app  (Apple Silicon build; run after
#             macos_app.sh, and before the final ad hoc `codesign --deep`, which signs it again
#             without its sandbox, so YOBUKO can start it with its own profile folder)
#   Windows:  <package>/mixxx/mixxx.exe and the rest of Mixxx's program folder
#   Linux:    nothing: Mixxx publishes no Linux build of its own, so YOBUKO uses the Mixxx of
#             the distribution (the .deb recommends the `mixxx` package)
#
# A VERSION note goes beside it (YOBUKO picks the DDJ-FLX4 mapping by it), and Mixxx's licence
# and where its source is go into the package. Checksums of the downloads are pinned in
# scripts/mixxx.sha256; a download not listed there is reported with its checksum, to pin.
#
# Usage: scripts/mixxx_fetch.sh <package-folder>
set -euo pipefail

MIXXX_VERSION="${MIXXX_VERSION:-2.5.6}"
BASE="https://downloads.mixxx.org/releases/$MIXXX_VERSION"

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <package-folder>" >&2
  exit 2
fi
package=$(cd "$1" && pwd)
here="$(cd "$(dirname "$0")" && pwd)"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}

fetch() {
  local name=$1
  curl --fail --location --retry 4 --retry-delay 2 --silent --show-error -o "$work/$name" "$BASE/$name"
  local got pinned
  got=$(sha256 "$work/$name")
  pinned=$(awk -v n="$name" '$2 == n {print $1}' "$here/mixxx.sha256" 2>/dev/null || true)
  if [ -z "$pinned" ]; then
    echo "::warning::$name is not pinned in scripts/mixxx.sha256; its SHA-256 is $got" >&2
  elif [ "$pinned" != "$got" ]; then
    echo "$name: SHA-256 $got, expected $pinned" >&2
    exit 1
  fi
}

licence() {
  cat > "$package/LICENSE-Mixxx.txt" <<EOF
Mixxx $MIXXX_VERSION, by the Mixxx team and contributors: https://mixxx.org
Free software under the GNU General Public License, version 2 or later.
YOBUKO ships the official, unmodified release from $BASE/
Its complete source code: https://github.com/mixxxdj/mixxx/tree/$MIXXX_VERSION
(also at $BASE/mixxx-$MIXXX_VERSION-src.tar.gz).
EOF
}

case "$(uname -s)" in
  Darwin)
    if [ ! -d "$package/Suhina.app" ]; then
      echo "missing app bundle: $package/Suhina.app (run scripts/macos_app.sh first)" >&2
      exit 1
    fi
    name="mixxx-$MIXXX_VERSION-macosarm.dmg"
    fetch "$name"
    mount="$work/mount"
    mkdir -p "$mount"
    hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$work/$name" >/dev/null
    trap 'hdiutil detach "$mount" >/dev/null 2>&1 || true; rm -rf "$work"' EXIT
    helpers="$package/Suhina.app/Contents/Helpers"
    mkdir -p "$helpers"
    rm -rf "$helpers/Mixxx.app"
    ditto "$mount/Mixxx.app" "$helpers/Mixxx.app"
    hdiutil detach "$mount" >/dev/null
    xattr -dr com.apple.quarantine "$helpers/Mixxx.app" 2>/dev/null || true
    echo "$MIXXX_VERSION" > "$helpers/Mixxx.version"
    ;;
  MINGW* | MSYS* | CYGWIN*)
    name="mixxx-$MIXXX_VERSION-win64.msi"
    fetch "$name"
    # An administrative install unpacks the program folder without installing anything.
    unpacked="$work/unpacked"
    mkdir -p "$unpacked"
    MSYS2_ARG_CONV_EXCL='*' msiexec /a "$(cygpath -w "$work/$name")" /qn TARGETDIR="$(cygpath -w "$unpacked")"
    exe=$(find "$unpacked" -iname mixxx.exe -print -quit)
    if [ -z "$exe" ]; then
      echo "no mixxx.exe in $name" >&2
      exit 1
    fi
    rm -rf "$package/mixxx"
    cp -R "$(dirname "$exe")" "$package/mixxx"
    echo "$MIXXX_VERSION" > "$package/mixxx/VERSION"
    ;;
  *)
    echo "Linux: Mixxx comes from the distribution (the .deb recommends it); nothing to fetch."
    exit 0
    ;;
esac
licence
echo "Mixxx $MIXXX_VERSION is in $package"
