#!/usr/bin/env bash
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Runs a command with a private virtual X display that it reaches over TCP on 127.0.0.1.
#
# xvfb-run gives the command a display reached through /tmp/.X11-unix. On the owner's Linux
# runner (Ubuntu in WSL, where WSLg owns /tmp/.X11-unix) that socket refused every connection,
# so every test that opens a plug-in editor's window failed with "Connection refused". A TCP
# display with its own authority file does not depend on that folder.
#
# Usage: scripts/self-hosted/with-xvfb.sh COMMAND [ARGS...]
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: $0 COMMAND [ARGS...]" >&2
  exit 2
fi

work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/yobuko-xvfb.XXXXXX")
export XAUTHORITY="$work/Xauthority"
touch "$XAUTHORITY"

# The first display number whose TCP port is free; two runners on one machine each find their own.
display=99
while [ -e "/tmp/.X${display}-lock" ] || (exec 3<>"/dev/tcp/127.0.0.1/$((6000 + display))") 2>/dev/null; do
  display=$((display + 1))
  if [ "$display" -gt 199 ]; then
    echo "with-xvfb: no free X display between :99 and :199" >&2
    exit 1
  fi
done

cookie=$(mcookie)
xauth -q -f "$XAUTHORITY" add "127.0.0.1:${display}" . "$cookie"
xauth -q -f "$XAUTHORITY" add ":${display}" . "$cookie"

Xvfb ":${display}" -screen 0 1280x1024x24 -noreset -auth "$XAUTHORITY" -listen tcp \
  >"$work/xvfb.log" 2>&1 &
xvfb=$!
cleanup() {
  kill "$xvfb" 2>/dev/null || true
  wait "$xvfb" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

# Wait until the server takes connections (at most 30 seconds).
for _ in $(seq 300); do
  if (exec 3<>"/dev/tcp/127.0.0.1/$((6000 + display))") 2>/dev/null; then
    break
  fi
  if ! kill -0 "$xvfb" 2>/dev/null; then
    echo "with-xvfb: Xvfb stopped:" >&2
    cat "$work/xvfb.log" >&2
    exit 1
  fi
  sleep 0.1
done

export DISPLAY="127.0.0.1:${display}"
echo "with-xvfb: DISPLAY=$DISPLAY"
set +e
"$@"
status=$?
set -e
exit "$status"
