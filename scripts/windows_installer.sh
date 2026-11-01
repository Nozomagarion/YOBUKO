#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Builds the Windows installer `YOBUKO-<label>-windows-setup.exe` from a package folder holding
# suhina.exe, suhina-cli.exe and suhina-plugin-host.exe, with scripts/windows_installer.iss.
# Runs in Git Bash on Windows; installs Inno Setup with Chocolatey when ISCC is not found.
#
# Usage: scripts/windows_installer.sh <package-folder> <version> <output-folder> <output-name>
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <package-folder> <version> <output-folder> <output-name>" >&2
  exit 2
fi
package=$(cd "$1" && pwd)
version=$2
mkdir -p "$3"
out=$(cd "$3" && pwd)
name=$4
here=$(cd "$(dirname "$0")" && pwd)

for bin in suhina suhina-cli suhina-plugin-host; do
  if [ ! -f "$package/$bin.exe" ]; then
    echo "missing executable: $package/$bin.exe" >&2
    exit 1
  fi
done

iscc=""
for candidate in "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" "/c/Program Files/Inno Setup 6/ISCC.exe"; do
  if [ -f "$candidate" ]; then iscc=$candidate; fi
done
if [ -z "$iscc" ]; then
  choco install innosetup -y --no-progress
  for candidate in "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" "/c/Program Files/Inno Setup 6/ISCC.exe"; do
    if [ -f "$candidate" ]; then iscc=$candidate; fi
  done
fi
if [ -z "$iscc" ]; then
  echo "Inno Setup's ISCC.exe was not found" >&2
  exit 1
fi

echo "Inno Setup compiler: $iscc"

# Windows' file version takes up to four numbers: 0.1.0 of 0.1.0-nightly.20261101032500+3612917.
short=${version%%[-+]*}

# Git Bash rewrites arguments that start with "/" into Windows paths ("/Q" would become "Q:/");
# ISCC's options start with "/", so path conversion is turned off for this one call.
MSYS2_ARG_CONV_EXCL='*' "$iscc" /Q \
  "/DAppVersion=$version" \
  "/DShortVersion=$short" \
  "/DSourceDir=$(cygpath -w "$package")" \
  "/DOutputDir=$(cygpath -w "$out")" \
  "/DOutputName=$name" \
  "$(cygpath -w "$here/windows_installer.iss")"
ls -l "$out/$name.exe"
