# Troubleshooting

First stop is always the daemon log:

```sh
journalctl --user -u gnome-notistack -f
```

## Notifications still look like GNOME's default banner

The takeover hasn't happened. Check:

```sh
systemctl --user status gnome-notistack            # active (running)?
gnome-extensions list --enabled | grep notistack   # extension enabled?
```

The extension frees the bus names a few seconds **after** the shell starts, so on
a fresh login give it ~10 s. If you just installed it, **log out and back in** —
the extension only loads at shell start on X11 (`Alt`+`F2` → `r` also reloads the
shell). Confirm ownership:

```sh
gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetNameOwner org.freedesktop.Notifications
```

## Nothing appears at all after login

The daemon may have started before X was ready. It self-heals `DISPLAY`/
`XAUTHORITY` from `systemctl --user show-environment` and retries, then exits so
systemd restarts it. Check the log for `connect to X11` errors and:

```sh
systemctl --user restart gnome-notistack
```

If it still can't connect, confirm you're on **X11** (not Wayland):
`echo $XDG_SESSION_TYPE` should print `x11`.

## Popups appear on the wrong monitor

Set `monitor` to a specific connector or to `focused`:

```sh
gnome-extensions prefs notistack@hemoglobina.store   # Layout → Monitor
# or:
gsettings set org.gnome.shell.extensions.notistack monitor 'DP-1'
```

List connector names with `xrandr --listmonitors`.

## "+N more" tile — clicking does nothing

Click anywhere on the tile card (not the gap above it — the spacing between
popups is dead space). Clicking it reveals all hidden notifications.

## Screen reader (Orca) doesn't speak notifications

1. Enable it: prefs → Behavior → **Announce to screen readers**, or
   `gsettings set org.gnome.shell.extensions.notistack a11y-announce true`.
2. Accessibility must be on: `gsettings set org.gnome.desktop.interface toolkit-accessibility true` and Orca running.
3. Restart the daemon after enabling. Verify the event is emitted:
   `journalctl --user -u gnome-notistack | grep AT-SPI`.

## Native-GTK / Flatpak app notifications are missing

Ensure `gtk-takeover` is on (default) and **restart the daemon** after changing it
(it re-serves the GTK interface on start, not live):

```sh
gsettings set org.gnome.shell.extensions.notistack gtk-takeover true
systemctl --user restart gnome-notistack
```

## Reset everything

```sh
gsettings reset-recursively org.gnome.shell.extensions.notistack
systemctl --user restart gnome-notistack
```
