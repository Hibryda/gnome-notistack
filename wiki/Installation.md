# Installation

> **Requirements:** GNOME Shell **48** on **X11**. Wayland is not supported.

After any install method, enable the service + extension and log out/in once so
the companion extension loads and frees the notification bus names.

## Debian / Ubuntu (`.deb`) — recommended

Download the `.deb` matching your distro from the **Releases** page.

```sh
sudo apt install ./gnome-notistack_<version>_<distro>.deb
systemctl --user enable --now gnome-notistack
gnome-extensions enable notistack@hemoglobina.store
```

The package installs the daemon to `/usr/lib/gnome-notistack/`, the systemd
**user** unit, and the GNOME extension (with its GSettings schema) to
`/usr/share/gnome-shell/extensions/`.

## Portable tarball (any X11 + GNOME 48 distro)

For non-Debian distros, use the portable tarball — a per-user install, no root:

```sh
tar xzf gnome-notistack-<version>-x86_64-<distro>.tar.gz
cd gnome-notistack-<version>-x86_64-<distro>
./install.sh
```

It installs into `~/.local/lib`, `~/.config/systemd/user`, and
`~/.local/share/gnome-shell/extensions`, compiles the schema, and enables both
the service and the extension. Remove it with `./install.sh --uninstall`.

Pick the tarball built on the **oldest** distro you can — a binary built against
an older glibc runs on newer systems, not the other way round.

## From source

```sh
sudo apt install build-essential pkg-config \
  libglib2.0-dev libcairo2-dev libpango1.0-dev libgdk-pixbuf-2.0-dev \
  libxcb1-dev libxcb-randr0-dev libxcb-shape0-dev libxcb-xfixes0-dev

git clone <repo-url> && cd gnome-notistack
cargo build --release
```

Then either build a `.deb` (`cargo install cargo-deb && cargo deb -p gnome-notistack`)
or install by hand — see [Building from source](How-It-Works) for the layout.
Targets **rustc 1.91+**.

## Verifying

```sh
systemctl --user status gnome-notistack         # should be active (running)
notify-send "Hello" "It works"                  # a stacked popup should appear
journalctl --user -u gnome-notistack -f         # live logs
```

If the popup looks like GNOME's default banner, the takeover hasn't happened yet
— see [Troubleshooting](Troubleshooting).
