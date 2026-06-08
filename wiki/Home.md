# gnome-notistack wiki

A notification daemon for **GNOME Shell 48 on X11** that renders a vertical
**stack** of notifications instead of GNOME's single banner — without dropping any
notifications the shell would otherwise show.

## Pages

- **[Installation](Installation)** — `.deb`, portable tarball, or from source.
- **[Configuration](Configuration)** — every setting, with examples.
- **[How It Works](How-It-Works)** — the bus-name takeover and rendering model.
- **[Troubleshooting](Troubleshooting)** — when notifications aren't taken over, login issues, multi-monitor, screen readers.

## At a glance

- Stacked, translucent popups for both `org.freedesktop.Notifications` and `org.gtk.Notifications`.
- Icons (incl. SVG), Pango markup, hyperlinks, inline images, action buttons, hover.
- Configurable placement (corners + centres; bottom stacks upward), theme-aware, geometry-based width.
- DND / lock / fullscreen suppression with replay; date-menu mirror; sounds; history.
- AT-SPI screen-reader announcements (opt-in).
- Live-configurable from a GTK preferences window.

> **Requirements:** GNOME Shell 48, X11 (no Wayland), the mutter compositor.

See the project's **README** for screenshots and the full feature list.
