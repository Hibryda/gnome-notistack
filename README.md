# gnome-notistack 🔔

A standalone notification daemon for **GNOME Shell 48 / X11** that shows **multiple stacked notifications at once** — replacing gnome-shell's one-banner-at-a-time display — without losing any notifications you'd otherwise see.

> **Status:** working with **full dual-name coverage**, validated live on GNOME 48.7/X11. Both `org.freedesktop.Notifications` (`notify-send`) and `org.gtk.Notifications` (native-GTK/Flatpak) traffic render as a live vertical stack of popups — icons, markup, urgency-aware expiry, fade in/out, click-to-dismiss, close/action signals, DND/lock suppression, sound; CLI/TOML-configurable. Both bus-name takeovers are validated and cleanly reversible (GTK with no shell restart needed to restore). Remaining: GTK buttons/action-dispatch (M7.1), packaging finalize, history/a11y.
> See [`docs/IMPLEMENTATION-PLAN.md`](docs/IMPLEMENTATION-PLAN.md) for milestone status, [`docs/gnome48-audit.md`](docs/gnome48-audit.md) for the takeover audit, and [`RESEARCH-BRIEF.md`](RESEARCH-BRIEF.md) for the design synthesis.

## The idea
GNOME Shell's `MessageTray` is hard-coded to display exactly one notification banner at a time; the rest queue. `gnome-notistack` takes over the notification D-Bus names and renders its own **vertical stack** of override-redirect popups (the dunst experience, on GNOME), with a companion GNOME extension to free the names from the shell.

## Stack
Rust · `zbus 5` · `x11rb 0.13` + `cairo`/`pango` · a companion GNOME-48 shell extension.

## Try it (dev)
```sh
cargo build                                    # daemon/ (workspace)
./target/debug/gnome-notistack --demo-popup    # render a sample popup (no takeover)
```
Running the full daemon requires freeing the bus name from gnome-shell — the
companion extension does this; see [`packaging/README.md`](packaging/README.md).
The Fdo takeover is validated and recoverable. Targets **rustc 1.91** (gtk-rs
pinned to the 0.21 line).

## Configuration
Zero-config by default. Override via a TOML file at
`~/.config/gnome-notistack/config.toml` (see [`config.example.toml`](config.example.toml))
or CLI flags (flags win over the file). Key options:

```sh
gnome-notistack --font "Cantarell" --summary-size 13 --body-size 11 --fade-ms 200
gnome-notistack --width 420 --max-stack 6 --default-timeout-ms 8000
gnome-notistack --config /path/to/config.toml
gnome-notistack --help     # full list
```

Configurable: font family, summary/body font sizes, fade duration, popup width,
margin/gap, max stack, default/low-urgency timeouts.

## License
MIT (planned).
