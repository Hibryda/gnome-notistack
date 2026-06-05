# Packaging

Artifacts that install the daemon, its D-Bus activation, and the companion
extension. Built into a `.deb` via `cargo deb` (see `daemon/Cargo.toml`
`[package.metadata.deb]`).

| File | Installed to | Purpose |
|---|---|---|
| `gnome-notistack.service` | `usr/lib/systemd/user/` | systemd **user** unit, `Type=dbus`, `BusName=org.freedesktop.Notifications`, crash-safe `ExecStopPost` cleanup |
| `dbus-1/org.freedesktop.Notifications.service` | `usr/share/dbus-1/services/` | D-Bus activation → our systemd unit |
| `../extension/*` | `usr/share/gnome-shell/extensions/notistack@hemoglobina.store/` | the mandatory takeover companion |

## Build the package
```sh
cargo install cargo-deb        # once
cargo deb -p gnome-notistack   # from repo root; output in target/debian/
```

## Manual dev install (no .deb)
```sh
cargo build --release
install -Dm755 target/release/gnome-notistack ~/.local/lib/gnome-notistack/gnome-notistack
# extension:
ln -s "$PWD/extension" ~/.local/share/gnome-shell/extensions/notistack@hemoglobina.store
gnome-extensions enable notistack@hemoglobina.store   # after a shell reload
```

> ⚠ The extension performs the bus-name takeover. The **GTK** path
> (`org.gtk.Notifications`) is gated OFF (`ALLOW_GTK_TAKEOVER = false` in
> `extension.js`) until the **M0.5 live spike** confirms mid-session
> `ReleaseName` keeps the shell stable. See `docs/gnome48-audit.md`.

## Recovery (if notifications break)
```sh
rm -rf ~/.config/systemd/user/org.gnome.Shell.Notifications.service.d \
       ~/.local/share/dbus-1/services/org.gnome.Shell.Notifications.service
systemctl --user daemon-reload
# then log out / in, or restart gnome-shell (Alt+F2 → r on X11)
```
