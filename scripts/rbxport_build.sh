#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Builds rbxport (Chris Le's DJ library app, vendored with YOBUKO's patches in
# third_party/rbxport-app) and puts it where YOBUKO looks for it, so DJ mode opens it
# (crates/ui/src/app/dj_rbxport.rs › bundled):
#
#   macOS:          <package>/Suhina.app/Contents/Helpers/rbxport.app  (run after macos_app.sh)
#   Windows/Linux:  <package>/rbxport/rbxport[.exe]
#
# Needs Node.js with corepack (pnpm comes from package.json's packageManager), and on Linux the
# WebKitGTK 4.1 development packages. rbxport's rust-toolchain.toml picks its own Rust.
#
# Usage: scripts/rbxport_build.sh <package-folder>
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <package-folder>" >&2
  exit 2
fi
package=$(cd "$1" && pwd)
source="$(cd "$(dirname "$0")/.." && pwd)/third_party/rbxport-app"

cd "$source"
corepack enable
pnpm install --frozen-lockfile

case "$(uname -s)" in
  Darwin)
    if [ ! -d "$package/Suhina.app" ]; then
      echo "missing app bundle: $package/Suhina.app (run scripts/macos_app.sh first)" >&2
      exit 1
    fi
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    pnpm tauri build --ci --target universal-apple-darwin --bundles app
    built="target/universal-apple-darwin/release/bundle/macos/rbxport.app"
    mkdir -p "$package/Suhina.app/Contents/Helpers"
    rm -rf "$package/Suhina.app/Contents/Helpers/rbxport.app"
    cp -R "$built" "$package/Suhina.app/Contents/Helpers/rbxport.app"
    ;;
  MINGW* | MSYS* | CYGWIN*)
    pnpm tauri build --ci --no-bundle
    mkdir -p "$package/rbxport"
    cp target/release/rbxport.exe "$package/rbxport/rbxport.exe"
    ;;
  *)
    pnpm tauri build --ci --no-bundle
    mkdir -p "$package/rbxport"
    cp target/release/rbxport "$package/rbxport/rbxport"
    chmod 0755 "$package/rbxport/rbxport"
    ;;
esac
cp LICENSE "$package/LICENSE-rbxport"
cp crates/rbl-deck/vendor/rubberband/COPYING "$package/LICENSE-rbxport-GPL-2.0.txt"
echo "rbxport is in $package"
