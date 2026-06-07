# gnome-notistack 🔔

A standalone notification daemon for **GNOME Shell 48 / X11** that shows **multiple stacked notifications at once** — replacing gnome-shell's one-banner-at-a-time display — without losing any notifications you'd otherwise see.

> **Status:** feature-complete (M0–M9 core), validated live on GNOME 48.7/X11. Both `org.freedesktop.Notifications` (`notify-send`) and `org.gtk.Notifications` (native-GTK/Flatpak) traffic render as a live vertical stack of popups — icons (PNG/JPEG/SVG/inline data), Pango markup, dynamic height, urgency-aware expiry, replaces_id, click-to-dismiss + action dispatch, close/action signals, fade in/out, DND/lock/fullscreen suppression, history, sound; GSettings-configurable (theme-aware, live, GTK prefs window), `cargo deb`-packaged. Both bus-name takeovers are validated and cleanly reversible (GTK with no shell restart needed to restore). Deferred (see [`docs/known-loss.md`](docs/known-loss.md)): a11y/AT-SPI2, per-button rendering, perf optimizations.
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
Zero-config by default — font and colors follow the **system theme**. Everything
is stored in **GSettings** (schema `org.gnome.shell.extensions.notistack`) and
edited from a **GTK preferences window**:

```sh
gnome-extensions prefs notistack@hemoglobina.store   # or the Extensions app
```

Changes apply **live** (the daemon re-reads GSettings ~1×/s and re-renders
on-screen popups) — except *take over native-GTK*, which re-serves on the next
daemon start. From the CLI you can also use `gsettings`:

```sh
gsettings set org.gnome.shell.extensions.notistack theme-mode 'dark'
gsettings set org.gnome.shell.extensions.notistack font-family 'Cantarell'
```

Configurable:
- **Appearance** — color theme (auto/light/dark, Adwaita palette), background/text
  color overrides, font family + title/body sizes (`0`/empty = from the system),
  title↔body gap.
- **Layout** — target monitor (`primary` or a connector like `DP-1`), width as a
  fraction of monitor *height* capped by a fraction of its *width* (or an absolute
  px), gap, margin, max stack. Popups start under the top bar (`_NET_WORKAREA`).
- **Timing** — default / low-urgency timeouts, a min and max display time (`0` =
  respect each notification's own timeout; min raises short timeouts, max
  force-closes long/"never expire" ones; min ≥ max gives a constant display
  time = max), fade duration.
- **Behavior** — history size, suppress-on-fullscreen, native-GTK takeover.

## Notification content
Popups render the full FDO/GTK notification: icon/image, Pango markup
(`<b> <i> <u>`), `<a href>` **hyperlinks** (clickable → opens in the browser),
inline `<img>` **images**, line breaks (`<br>` and `\n`), and **action buttons**
(FDO `actions` / GTK `buttons`) — clicking a button invokes its action; clicking
the card invokes the default action. Urgency-aware expiry, `replaces_id`, sounds,
and DND/lock/fullscreen suppression apply throughout.

## License
MIT (planned).
