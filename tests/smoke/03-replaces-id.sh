#!/usr/bin/env bash
# 03 — replaces_id updates a popup in place (no second card).
set -u
cd "$(dirname "$0")/../.." || exit 1
source tests/smoke/lib.sh
FAILED=0

takeover
ID=$(notify-send -p "Progress" "0%")
_delay 0.3
notify-send -p -r "$ID" "Progress" "50%" >/dev/null
notify-send -p -r "$ID" "Progress" "100%" >/dev/null
_delay 0.4
SHOWN=$(grep -c "popup shown" /tmp/notistack-smoke.log)
UPDATED=$(grep -c "popup updated in place" /tmp/notistack-smoke.log)
[ "$SHOWN" -eq 1 ] && pass "exactly one popup shown" || fail "expected 1 shown, got $SHOWN"
[ "$UPDATED" -ge 2 ] && pass "updated in place (got $UPDATED)" || fail "expected >=2 in-place updates, got $UPDATED"

rollback; trap - EXIT
exit "$FAILED"
