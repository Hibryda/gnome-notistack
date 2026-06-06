#!/usr/bin/env bash
# 02 — notify-send notifications render as a stack; one expires.
set -u
cd "$(dirname "$0")/../.." || exit 1
source tests/smoke/lib.sh
FAILED=0

takeover
notify-send "Smoke 1" "first"
notify-send "Smoke 2" "second"
notify-send -t 600 "Smoke 3" "expires fast"
_delay 0.5
SHOWN=$(grep -c "popup shown" /tmp/notistack-smoke.log)
[ "$SHOWN" -ge 3 ] && pass "3 popups shown (got $SHOWN)" || fail "expected >=3 popups, got $SHOWN"

# After the short timeout, Smoke 3 should have expired (closed reason 1).
_delay 1.2
log_has "popup closed" && pass "a popup expired" || fail "no expiry observed"

rollback; trap - EXIT
exit "$FAILED"
