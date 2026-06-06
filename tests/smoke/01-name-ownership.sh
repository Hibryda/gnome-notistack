#!/usr/bin/env bash
# 01 — the daemon takes over org.freedesktop.Notifications and the shell survives.
set -u
cd "$(dirname "$0")/../.." || exit 1
source tests/smoke/lib.sh
FAILED=0
SHELL_PID=$(pgrep -x gnome-shell | head -1)

takeover
OWNER_PID=$(_pid_of org.freedesktop.Notifications)
[ "$OWNER_PID" = "$DAEMON_PID" ] && pass "daemon owns org.freedesktop.Notifications" \
  || fail "expected daemon ($DAEMON_PID) to own the name, got pid $OWNER_PID"
kill -0 "$SHELL_PID" 2>/dev/null && pass "gnome-shell still alive" || fail "gnome-shell died"
gnome-extensions list >/dev/null 2>&1 && pass "gnome-shell still responsive" || fail "shell unresponsive"

rollback; trap - EXIT
_delay 0.4
RESTORED=$(_pid_of org.freedesktop.Notifications)
[ -n "$RESTORED" ] && [ "$RESTORED" != "$DAEMON_PID" ] && pass "name restored to the shell proxy" \
  || fail "name not restored (owner pid $RESTORED)"
exit "$FAILED"
