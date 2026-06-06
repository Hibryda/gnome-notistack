// handshake.js — the bus-name takeover, fully async (no main-thread blocking).
//
// Audit-driven mechanism (docs/gnome48-audit.md):
//   Fdo name (org.freedesktop.Notifications): held by a killable gjs subprocess.
//     → install blocking overrides, then SIGTERM the subprocess; the queued
//       daemon is promoted by D-Bus.
//   GTK name (org.gtk.Notifications): held by the MAIN gnome-shell on its shared
//     Gio.DBus.session connection, with the owner id DISCARDED — so
//     Gio.bus_unown_name() is impossible. We release it with
//     org.freedesktop.DBus.ReleaseName(...) from inside the shell, dispatched via
//     GLib.timeout_add(0) so it fires strictly AFTER enable() returns. The
//     daemon sits queued (REPLACE_EXISTING, no ALLOW_REPLACEMENT) → auto-promoted.
//
// SAFETY: the GTK release perturbs the live shell and is gated behind M0.5
// validation. Until `allowGtkTakeover` is enabled, only the (recoverable) Fdo
// path runs. See takeover() below.

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';

import * as ServiceOverride from './serviceOverride.js';

const DBUS = 'org.freedesktop.DBus';
const DBUS_PATH = '/org/freedesktop/DBus';
const FDO_NAME = 'org.freedesktop.Notifications';
const GTK_NAME = 'org.gtk.Notifications';
const SHELL_NOTIFICATIONS = 'org.gnome.Shell.Notifications';

const CONTROL_NAME = 'store.hemoglobina.notistack.Control';
const CONTROL_PATH = '/store/hemoglobina/notistack/Control';

function bus() {
    return Gio.DBus.session;
}

/** Promise-wrapped Gio.DBus.session.call. */
function dbusCall(dest, path, iface, method, params) {
    return new Promise((resolve, reject) => {
        bus().call(dest, path, iface, method, params, null,
            Gio.DBusCallFlags.NONE, -1, null, (conn, res) => {
                try {
                    resolve(conn.call_finish(res));
                } catch (e) {
                    reject(e);
                }
            });
    });
}

async function getNameOwner(name) {
    try {
        return (await dbusCall(DBUS, DBUS_PATH, DBUS,
            'GetNameOwner', new GLib.Variant('(s)', [name]))).deepUnpack()[0];
    } catch (_e) {
        return null; // NameHasNoOwner
    }
}

async function getNameOwnerPid(name) {
    const owner = await getNameOwner(name);
    if (!owner)
        return 0;
    return (await dbusCall(DBUS, DBUS_PATH, DBUS,
        'GetConnectionUnixProcessID', new GLib.Variant('(s)', [owner]))).deepUnpack()[0];
}

/**
 * Fast-path: the takeover is already done when our daemon owns the Fdo name —
 * i.e. the Fdo name and our control name resolve to the same connection. (The
 * daemon's IsReady only reports liveness, not ownership, so it can't be used here.)
 */
async function takeoverAlreadyDone() {
    const fdoOwner = await getNameOwner(FDO_NAME);
    const ctrlOwner = await getNameOwner(CONTROL_NAME);
    return fdoOwner !== null && fdoOwner === ctrlOwner;
}

function reportEvent(kind, detail) {
    // Fire-and-forget telemetry to the daemon's private control iface.
    bus().call(CONTROL_NAME, CONTROL_PATH, CONTROL_NAME, 'ReportHandshakeEvent',
        new GLib.Variant('(ss)', [kind, String(detail ?? '')]), null,
        Gio.DBusCallFlags.NONE, 2000, null, null);
}

/** SIGTERM the gjs subprocess that owns the Fdo names, via Gio.Subprocess (async). */
async function sigtermPid(pid) {
    const proc = Gio.Subprocess.new(['kill', '-TERM', String(pid)],
        Gio.SubprocessFlags.NONE);
    await new Promise((resolve, reject) => {
        proc.wait_async(null, (p, res) => {
            try {
                p.wait_finish(res);
                resolve();
            } catch (e) {
                reject(e);
            }
        });
    });
}

/** Phase A — free the Fdo name (recoverable; runs unconditionally). */
async function takeoverFdo() {
    await ServiceOverride.install();
    // daemon-reload so the systemd override takes effect before we kill the proxy.
    const reload = Gio.Subprocess.new(
        ['systemctl', '--user', 'daemon-reload'], Gio.SubprocessFlags.STDERR_PIPE);
    await new Promise((resolve, reject) => {
        reload.wait_check_async(null, (p, res) => {
            try {
                p.wait_check_finish(res);
                resolve();
            } catch (e) {
                reject(e);
            }
        });
    });
    const pid = await getNameOwnerPid(FDO_NAME);
    await sigtermPid(pid);
    reportEvent('FDO_RELEASED', `pid=${pid}`);
}

/** Phase B — release the GTK name from inside the shell (gated; perturbs shell). */
async function takeoverGtk() {
    // Hygiene: unexport the in-process object path (does NOT release the name).
    // The name release itself, dispatched after enable() returns:
    await new Promise(resolve => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 0, () => {
            bus().call(DBUS, DBUS_PATH, DBUS, 'ReleaseName',
                new GLib.Variant('(s)', [GTK_NAME]), null,
                Gio.DBusCallFlags.NONE, -1, null, (conn, res) => {
                    try {
                        conn.call_finish(res);
                        reportEvent('GTK_RELEASED', GTK_NAME);
                    } catch (e) {
                        reportEvent('GTK_RELEASE_FAILED', e.message);
                    }
                });
            resolve();
            return GLib.SOURCE_REMOVE;
        });
    });
}

/**
 * Run the takeover.
 * @param {{allowGtkTakeover: boolean}} opts
 */
export async function takeover(opts) {
    if (await takeoverAlreadyDone()) {
        reportEvent('FASTPATH', 'daemon already owns the Fdo name');
        return;
    }
    await takeoverFdo();
    if (opts.allowGtkTakeover)
        await takeoverGtk();
    else
        reportEvent('GTK_SKIPPED', 'allowGtkTakeover=false');
}

async function runReload() {
    const proc = Gio.Subprocess.new(
        ['systemctl', '--user', 'daemon-reload'], Gio.SubprocessFlags.NONE);
    await new Promise(resolve => {
        proc.wait_async(null, (p, res) => {
            try { p.wait_finish(res); } catch (_e) {}
            resolve();
        });
    });
}

/**
 * Restore shell ownership on disable() — no Meta.restart (validated).
 * Sequence: ask the daemon to release the names (it stays running), reactivate
 * the gjs Fdo proxy, then re-own the GTK name for the shell (its object path was
 * never unexported). See docs/gnome48-audit.md "GTK path".
 */
export async function restore({ gtkWasTakenOver }) {
    // 1. Daemon releases org.freedesktop.Notifications + org.gtk.Notifications.
    await dbusCall(CONTROL_NAME, CONTROL_PATH, CONTROL_NAME, 'Relinquish', null)
        .catch(e => reportEvent('RELINQUISH_FAILED', e.message));

    // 2. Restore Fdo: drop the blocking overrides, reload, reactivate the proxy.
    await ServiceOverride.remove();
    await runReload();
    await dbusCall(DBUS, DBUS_PATH, DBUS, 'StartServiceByName',
        new GLib.Variant('(su)', [SHELL_NOTIFICATIONS, 0])).catch(() => {});

    // 3. Restore GTK: re-own the freed name for the shell.
    if (gtkWasTakenOver)
        Gio.DBus.session.own_name(GTK_NAME, Gio.BusNameOwnerFlags.REPLACE, null, null);

    return { needsShellRestart: false };
}
