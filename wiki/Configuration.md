# Configuration

Everything is stored in GSettings (`org.gnome.shell.extensions.notistack`) and
edited from the preferences window:

```sh
gnome-extensions prefs notistack@hemoglobina.store
```

Changes apply **live** — the daemon re-reads GSettings about once a second and
re-renders on-screen popups. The one exception is `gtk-takeover`, which re-serves
on the next daemon start. You can also script settings with `gsettings`.

## Appearance

| Key | Default | Notes |
|---|---|---|
| `theme-mode` | `auto` | `auto` follows the system light/dark; or force `light`/`dark`. |
| `background-color` | `''` | `#rrggbb` or `#rrggbbaa`; empty = from theme. |
| `foreground-color` | `''` | Text colour override; empty = from theme. |
| `font-family` | `''` | Empty = the system interface font. |
| `summary-size-pt` / `body-size-pt` | `0` | `0` = derived from the system font size. |
| `title-body-gap-px` | `4` | Gap between title and body. |

## Layout

| Key | Default | Notes |
|---|---|---|
| `placement` | `top-right` | `top-left`, `top-center`, `top-right`, `bottom-left`, `bottom-center`, `bottom-right`. **Bottom positions stack upward** (newest at the bottom). |
| `monitor` | `primary` | `primary`, `focused` (monitor under the pointer when a fresh batch appears), or a RandR connector like `DP-1`. |
| `width-height-fraction` | `0.30` | Width = this fraction of monitor **height**… |
| `max-width-fraction` | `0.18` | …capped at this fraction of monitor **width** (good for ultrawides). |
| `width-px` | `0` | A non-zero value overrides the geometry-based width. |
| `gap-px` | `10` | Vertical gap between cards. |
| `margin-px` | `16` | Distance from the screen edge. |
| `max-stack` | `5` | Beyond this, extra notifications collapse into a clickable "+N more" tile. |

Popups always clear panels/docks (they honour `_NET_WORKAREA`).

## Timing

| Key | Default | Notes |
|---|---|---|
| `default-timeout-ms` | `5000` | Used when an app doesn't request a timeout. |
| `low-urgency-timeout-ms` | `3000` | For low-urgency notifications. |
| `min-timeout-ms` | `0` | Floor — show for *at least* this long. `0` = respect each notification. |
| `max-timeout-ms` | `0` | Ceiling — force-close after this, including "never expire" (critical) popups. `0` = off. If `min ≥ max > 0`, every popup shows for a constant `max`. |
| `fade-ms` | `150` | Fade in/out duration; `0` disables fading. |

## Behavior

| Key | Default | Notes |
|---|---|---|
| `history-size` | `100` | Notifications kept in the persisted history. |
| `suppress-on-fullscreen` | `true` | Hide while a fullscreen window is focused (queued + replayed after). |
| `gtk-takeover` | `true` | Also handle `org.gtk.Notifications` (native-GTK/Flatpak). Restart the daemon to change. |
| `a11y-announce` | `false` | Emit AT-SPI announcements so Orca speaks notifications. Needs accessibility enabled. |

## Examples

```sh
N=org.gnome.shell.extensions.notistack
gsettings set $N placement 'bottom-right'      # bottom-right, stacks upward
gsettings set $N monitor 'focused'             # follow the active monitor
gsettings set $N theme-mode 'dark'
gsettings set $N max-timeout-ms 15000          # nothing stays longer than 15s
gsettings set $N a11y-announce true            # screen-reader announcements
```

DND uses GNOME's own toggle (`org.gnome.desktop.notifications show-banners`), so
the system Do-Not-Disturb suppresses notistack too.
