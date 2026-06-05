# gnome-notistack 🔔

A standalone notification daemon for **GNOME Shell 48 / X11** that shows **multiple stacked notifications at once** — replacing gnome-shell's one-banner-at-a-time display — without losing any notifications you'd otherwise see.

> **Status:** planning / research. No code yet.
> See [`RESEARCH-BRIEF.md`](RESEARCH-BRIEF.md) for the synthesized design, and [`research/`](research/) for the per-dimension deep dives.

## The idea
GNOME Shell's `MessageTray` is hard-coded to display exactly one notification banner at a time; the rest queue. `gnome-notistack` takes over the notification D-Bus names and renders its own **vertical stack** of override-redirect popups (the dunst experience, on GNOME), with a companion GNOME extension to free the names from the shell.

## Stack
Rust · `zbus` · `x11rb` + `cairo`/`pango` · a companion GNOME-48 shell extension.

## License
MIT (planned).
