#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Sets up the owner's own Linux build machine: Ubuntu 22.04 running in WSL on their Windows PC,
# registered with GitHub as two self-hosted runners labelled `yobuko-linux`. The YOBUKO
# workflows run their Linux jobs there (tests, AppImage and .deb), so GitHub's own machines and
# minutes are not used. Two runners, so the Internet test's host and its Linux guest run side by
# side.
#
# Run as root inside the WSL distribution, with a registration token from
# https://github.com/Nozomagarion/YOBUKO/settings/actions/runners/new (valid one hour):
#
#   bash yobuko-linux-runner.sh <token>
#
# scripts/self-hosted/brancher-yobuko-linux.ps1 does all of it from Windows. Running it again
# updates the tools and registers the runners anew.
set -euo pipefail

if [ "$#" -ne 1 ] || [ -z "$1" ]; then
  echo "usage: $0 <registration-token>" >&2
  exit 2
fi
token=$1
repo_url=https://github.com/Nozomagarion/YOBUKO
user=runner
export DEBIAN_FRONTEND=noninteractive

if [ "$(id -u)" -ne 0 ]; then
  echo "run this as root" >&2
  exit 1
fi

echo "== Ubuntu packages (what GitHub's Ubuntu 22.04 machines had for these jobs)"
apt-get update
apt-get install -y --no-install-recommends \
  build-essential ca-certificates curl git git-lfs gnupg jq sudo file zip unzip xz-utils \
  pkg-config cmake clang libclang-dev perl python3 python3.11 python3-pip python3-venv dpkg-dev \
  libasound2-dev libfontconfig1-dev libssl-dev xvfb xauth imagemagick \
  libwebkit2gtk-4.1-dev librsvg2-dev libgtk-3-dev libayatana-appindicator3-dev patchelf \
  libgl1-mesa-dri libegl1 libxkbcommon-x11-0 libxkbcommon-dev libx11-dev libxcursor1 libxi6 \
  libxrandr2 libwayland-client0 libicu70 locales tzdata
locale-gen en_US.UTF-8 >/dev/null

echo "== GitHub CLI (the Internet test and the release publish call it)"
if ! command -v gh >/dev/null 2>&1; then
  curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    -o /usr/share/keyrings/githubcli-archive-keyring.gpg
  echo "deb [arch=amd64 signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    > /etc/apt/sources.list.d/github-cli.list
  apt-get update
  apt-get install -y gh
fi

echo "== The '$user' account the jobs run as"
if ! id "$user" >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash "$user"
fi
# Jobs only install a missing Ubuntu package with sudo; nothing else.
echo "$user ALL=(root) NOPASSWD: /usr/bin/apt-get" > /etc/sudoers.d/yobuko-runner
chmod 0440 /etc/sudoers.d/yobuko-runner

# The jobs start as `runner`, see Linux tools only (not the PC's Windows programs on the PATH),
# and keep Linux line endings.
cat > /etc/wsl.conf <<CONF
[user]
default=$user

[interop]
appendWindowsPath=false
CONF

home=$(getent passwd "$user" | cut -d: -f6)
as_user() { sudo -u "$user" -H bash -lc "$1"; }

echo "== Rust (rustup, stable, with rustfmt and clippy)"
if [ ! -x "$home/.cargo/bin/rustup" ]; then
  as_user 'curl --proto =https --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable -c rustfmt -c clippy'
else
  as_user 'rustup update stable && rustup component add rustfmt clippy'
fi

echo "== GitHub Actions runner"
version=$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/actions/runner/releases/latest | sed 's#.*/v##')
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "could not find the latest runner version (got '$version')" >&2; exit 1 ;;
esac
archive="$home/actions-runner-linux-x64-$version.tar.gz"
if [ ! -f "$archive" ]; then
  curl -fsSL -o "$archive" \
    "https://github.com/actions/runner/releases/download/v$version/actions-runner-linux-x64-$version.tar.gz"
  chown "$user:$user" "$archive"
fi

for i in 1 2; do
  dir="$home/actions-runner-$i"
  mkdir -p "$dir"
  # Registered before: forget it here; --replace below takes its place on GitHub.
  rm -f "$dir/.runner" "$dir/.credentials" "$dir/.credentials_rsaparams"
  tar -xzf "$archive" -C "$dir"
  chown -R "$user:$user" "$dir"
  "$dir/bin/installdependencies.sh" >/dev/null
  as_user "cd '$dir' && ./config.sh --unattended --replace --url '$repo_url' --token '$token' \
    --name 'yobuko-linux-$i' --labels yobuko-linux --work _work"
done

# What Windows starts at each sign-in (the scheduled task "YOBUKO Linux runner"): both runners,
# for as long as the PC is on.
cat > "$home/start-runners.sh" <<'RUN'
#!/usr/bin/env bash
cd "$HOME"
# Rust's tools on the PATH, as in a sign-in shell.
. "$HOME/.cargo/env"
for i in 1 2; do
  (cd "$HOME/actions-runner-$i" && ./run.sh >> "$HOME/actions-runner-$i/runner.log" 2>&1) &
done
wait
RUN
chmod 0755 "$home/start-runners.sh"
chown "$user:$user" "$home/start-runners.sh"

echo
echo "YOBUKO Linux runners ready: yobuko-linux-1 and yobuko-linux-2."
