# Packaging

Artifacts that install the daemon + companion extension, built into a `.deb` via
`cargo deb` (see `daemon/Cargo.toml` `[package.metadata.deb]`).

| File | Installed to | Purpose |
|---|---|---|
| `gnome-notistack.service` | `usr/lib/systemd/user/` | systemd **user** unit, `Type=simple`, long-running + autostart at `graphical-session.target` |
| `../extension/*` | `usr/share/gnome-shell/extensions/notistack@hemoglobina.store/` | the mandatory takeover companion |

**Model:** the daemon runs as a long-running user service that **starts queued**
(requests both notification names `REPLACE_EXISTING`, lands `InQueue`). The
extension — deferred ~4s past shell-init and **only once the daemon is ready** —
SIGTERMs the gjs proxy (frees `org.freedesktop.Notifications`) and `ReleaseName`s
`org.gtk.Notifications` from inside the shell; D-Bus promotes the queued daemon.
There is **no `/bin/false` override** (the queue model prevents re-grab; the
override caused a login SIGSEGV — see `docs/known-loss.md`).

## Build the package
```sh
cargo install cargo-deb        # once
cargo deb -p gnome-notistack   # from repo root; output in target/debian/
```

## Manual dev install (no .deb)
```sh
cargo build --release
install -Dm755 target/release/gnome-notistack ~/.local/lib/gnome-notistack/gnome-notistack

# systemd user unit (autostart, queued):
install -Dm644 packaging/gnome-notistack.service ~/.config/systemd/user/gnome-notistack.service
# (point ExecStart at ~/.local/lib for a no-sudo install, or use the /usr/lib deb path)
systemctl --user daemon-reload
systemctl --user enable --now gnome-notistack.service

# extension:
cp extension/extension.js extension/handshake.js extension/metadata.json \
   ~/.local/share/gnome-shell/extensions/notistack@hemoglobina.store/
# restart gnome-shell (Alt+F2 → r on X11) so it discovers the extension, then:
gnome-extensions enable notistack@hemoglobina.store
```

> The takeover is **gated on the daemon being up** and **deferred** past
> shell-init, so enabling at login is safe (the daemon autostarts queued first).
> If the daemon isn't running, the extension does nothing (no destabilization).

## Recovery (if notifications ever break)
The daemon owns the names non-replaceably, so the worst case is "names unowned"
(e.g. daemon stopped while the extension is enabled), not a shell crash. To reset:
```sh
gnome-extensions disable notistack@hemoglobina.store   # hands back to the shell
systemctl --user restart gnome-notistack.service       # or restart the daemon
# if the shell's own proxy needs waking:
gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.StartServiceByName org.gnome.Shell.Notifications 0
```
