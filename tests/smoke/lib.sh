#!/usr/bin/env bash
# Shared helpers for the gnome-notistack smoke tests.
#
# These are INTEGRATION tests: they take over org.freedesktop.Notifications on
# the live session (SIGTERM the gjs proxy, queue-promote the daemon), run
# assertions, then roll back. Recoverable on X11. Run one at a time.
#
# Usage (from repo root):  bash tests/smoke/02-fdo-stack.sh
set -u

DAEMON_BIN="${DAEMON_BIN:-./target/debug/gnome-notistack}"
SHELL_SVC="$HOME/.local/share/dbus-1/services/org.gnome.Shell.Notifications.service"
DAEMON_PID=""

_owner() { gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetNameOwner "$1" 2>&1; }
_pid_of() {
  local o; o=$(_owner "$1" | sed -E "s/.*'(:[0-9.]+)'.*/\1/")
  case "$o" in :*) gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID "$o" 2>/dev/null | sed -E 's/.*uint32 ([0-9]+).*/\1/';; esac
}
_delay() { timeout "$1" tail -f /dev/null 2>/dev/null || true; }

pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1"; FAILED=1; }

# Start the daemon (queued) and take over the Fdo name via the proxy SIGTERM.
takeover() {
  : > /tmp/notistack-smoke.log
  RUST_LOG=info nohup "$DAEMON_BIN" >> /tmp/notistack-smoke.log 2>&1 &
  DAEMON_PID=$!
  _delay 1.0
  local gjs; gjs=$(_pid_of org.freedesktop.Notifications)
  printf '[D-BUS Service]\nName=org.gnome.Shell.Notifications\nExec=/bin/false\n' > "$SHELL_SVC"
  [ -n "$gjs" ] && [ "$gjs" != "$DAEMON_PID" ] && kill -TERM "$gjs"
  local i; for i in $(seq 1 30); do _owner org.freedesktop.Notifications | grep -q "$gjs" && _delay 0.2 || break; done
}

# Stop the daemon, remove the block, reactivate the shell's gjs proxy.
rollback() {
  [ -n "$DAEMON_PID" ] && kill -TERM "$DAEMON_PID" 2>/dev/null
  _delay 0.4
  rm -f "$SHELL_SVC"
  gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.StartServiceByName org.gnome.Shell.Notifications 0 >/dev/null 2>&1
  rm -f /tmp/notistack-smoke.log
}
trap rollback EXIT

log_has() { grep -q "$1" /tmp/notistack-smoke.log; }
