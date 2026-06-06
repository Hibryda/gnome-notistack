# Smoke tests

Integration smoke tests for the live daemon. **They take over
`org.freedesktop.Notifications` on your running GNOME/X11 session** (SIGTERM the
gjs proxy, queue-promote the daemon), assert, then roll back (recoverable on X11).

Run **one at a time** from the repo root after `cargo build`:

```sh
bash tests/smoke/01-name-ownership.sh   # takeover + shell survives + restore
bash tests/smoke/02-fdo-stack.sh        # notify-send → stacked popups + expiry
bash tests/smoke/03-replaces-id.sh      # replaces_id updates in place
```

Each prints `PASS:`/`FAIL:` lines and exits non-zero on failure. `lib.sh` holds
the takeover/rollback helpers; an `EXIT` trap rolls back even on early failure.

> These perturb the live session briefly (FDO notifications route to the daemon
> during the test). GTK-path tests require the companion extension loaded (a
> shell restart) and are covered by `docs/gnome48-audit.md`.
