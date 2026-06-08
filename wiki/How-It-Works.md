# How it works

GNOME Shell's `MessageTray` is hard-coded to show one banner at a time; the rest
queue. `gnome-notistack` replaces that display with its own stack of popups.

## The bus-name takeover

Notifications arrive on two D-Bus names, both owned by the shell:

- `org.freedesktop.Notifications` — the FreeDesktop spec (most apps, `notify-send`).
- `org.gtk.Notifications` — native-GTK / Flatpak apps, held by the shell process.

The daemon and a companion shell extension cooperate:

1. The **daemon** starts at login (systemd user unit) and requests both names with
   `REPLACE_EXISTING` only — so it lands **queued** behind the shell, owning
   nothing yet. Starting queued *before* the takeover is what makes login safe.
2. A few seconds after the shell settles, the **extension** frees the names:
   it SIGTERMs the gjs proxy that owns `org.freedesktop.Notifications`, and calls
   `ReleaseName('org.gtk.Notifications')` from inside the shell.
3. D-Bus promotes the queued daemon to owner. From then on, all notifications come
   to the daemon.

On `disable()`, the extension hands the names back — the daemon relinquishes them
and the shell re-acquires `org.gtk.Notifications` and reactivates its FDO proxy,
**without a shell restart**. If the daemon is restarted mid-session, the extension
notices and re-runs the takeover.

## Rendering

The daemon owns the X11 connection on a dedicated thread (cairo/X11 state isn't
`Send`). Each popup is an **override-redirect ARGB32 window** drawn with
cairo + pango: rounded translucent card, icon, title/body with markup, inline
images, a bottom action-bar, and clickable link/button regions. EWMH hints mark
the windows as above-everything notification windows that keep the compositor (so
alpha works). A running y-cursor lays the stack out without overlaps or gaps; the
configured placement decides the anchor edge and growth direction.

D-Bus handlers run on a tokio runtime and talk to the render thread over a
channel; closures/actions flow back over a feedback channel to an async task that
emits the `NotificationClosed` / `ActionInvoked` signals.

## Suppression

Do-Not-Disturb (`org.gnome.desktop.notifications`), screen lock
(`org.gnome.ScreenSaver`), and a focused fullscreen window each suppress popups.
Suppressed notifications are queued and replayed when the condition clears — none
are lost.

## Building / layout

- `daemon/` — the Rust daemon (`zbus` + `x11rb` + `cairo`/`pango`).
- `extension/` — the GNOME 48 shell extension (takeover + date-menu mirror + prefs).
- `packaging/` — the systemd unit, `cargo-deb` maintainer scripts, portable installer.

```sh
cargo build --release           # → target/release/gnome-notistack
cargo deb -p gnome-notistack    # → target/debian/*.deb (needs cargo-deb)
```
