// handshake.js — the bus-name takeover, fully async (no main-thread blocking).
//
// Audit-driven mechanism (docs/gnome48-audit.md):
//   Fdo name (org.freedesktop.Notifications): held by a killable gjs subprocess.
//     → SIGTERM the subprocess; our queued daemon (REPLACE_EXISTING, no
//       ALLOW_REPLACEMENT) is auto-promoted by D-Bus. No /bin/false override is
//       used: the daemon's non-replaceable ownership already prevents the proxy
//       re-grabbing the name, and the override caused login-time activation
//       failures + crashes (it is removed entirely).
//   GTK name (org.gtk.Notifications): held by the MAIN gnome-shell on its shared
//     Gio.DBus.session connection (owner id discarded → bus_unown_name impossible).
//     → org.freedesktop.DBus.ReleaseName(...) from inside the shell, via
//       GLib.timeout_add(0); queued daemon auto-promoted.
//
// SAFETY: takeover() NEVER frees a name unless our daemon is up and ready to
// claim it (the precondition whose absence segfaulted the shell at login), and
// extension.js defers the whole thing out of the fragile shell-init window.

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';

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

function sleep(ms) {
    return new Promise(resolve => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            resolve();
            return GLib.SOURCE_REMOVE;
        });
    });
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
 * i.e. the Fdo name and our control name resolve to the same connection.
 */
async function takeoverAlreadyDone() {
    const fdoOwner = await getNameOwner(FDO_NAME);
    const ctrlOwner = await getNameOwner(CONTROL_NAME);
    return fdoOwner !== null && fdoOwner === ctrlOwner;
}

/** True only if our daemon is running and answering on its control interface. */
async function daemonReady() {
    try {
        const r = await dbusCall(CONTROL_NAME, CONTROL_PATH, CONTROL_NAME, 'IsReady', null);
        return r.deepUnpack()[0] === true;
    } catch (_e) {
        return false; // control name has no owner → daemon not running
    }
}

function reportEvent(kind, detail) {
    // Fire-and-forget telemetry to the daemon's private control iface.
    bus().call(CONTROL_NAME, CONTROL_PATH, CONTROL_NAME, 'ReportHandshakeEvent',
        new GLib.Variant('(ss)', [kind, String(detail ?? '')]), null,
        Gio.DBusCallFlags.NONE, 2000, null, null);
}

/** SIGTERM a pid via Gio.Subprocess. Guards against pid <= 1 (never kill 0 — that
 *  would signal gnome-shell's whole process group). */
async function sigtermPid(pid) {
    if (!pid || pid <= 1)
        return;
    const proc = Gio.Subprocess.new(['kill', '-TERM', String(pid)], Gio.SubprocessFlags.NONE);
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

/** Phase A — free the Fdo name: SIGTERM the gjs proxy; the queued daemon claims it. */
async function takeoverFdo() {
    const pid = await getNameOwnerPid(FDO_NAME);
    await sigtermPid(pid);
    reportEvent('FDO_RELEASED', `pid=${pid}`);
}

/** Phase B — release the GTK name from inside the shell (queued daemon promoted).
 *  Resolves to whether the name was actually released. */
async function takeoverGtk() {
    return new Promise(resolve => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 0, () => {
            bus().call(DBUS, DBUS_PATH, DBUS, 'ReleaseName',
                new GLib.Variant('(s)', [GTK_NAME]), null,
                Gio.DBusCallFlags.NONE, -1, null, (conn, res) => {
                    try {
                        conn.call_finish(res);
                        reportEvent('GTK_RELEASED', GTK_NAME);
                        resolve(true); // resolve AFTER completion (was: before)
                    } catch (e) {
                        reportEvent('GTK_RELEASE_FAILED', e.message);
                        resolve(false);
                    }
                });
            return GLib.SOURCE_REMOVE;
        });
    });
}

/**
 * Run the takeover. Never frees a name unless our daemon is up and ready to claim
 * it — the missing precondition that crashed the shell at login. Returns whether
 * the GTK name ended up taken over (so the caller knows whether restore() must
 * re-own it — setting that flag before this runs caused restore() to re-own a
 * name that was never freed).
 * @param {{allowGtkTakeover: boolean, cancelled?: () => boolean}} opts
 * @returns {Promise<boolean>}
 */
export async function takeover(opts) {
    const cancelled = opts.cancelled || (() => false);
    if (await takeoverAlreadyDone()) {
        reportEvent('FASTPATH', 'daemon already owns the Fdo name');
        return opts.allowGtkTakeover; // a prior run freed GTK iff takeover is on
    }
    // Gate: the daemon must be running/ready (it may be starting via systemd
    // concurrently). Retry briefly; if it never appears, do NOTHING — freeing the
    // names with no claimant is what segfaulted the shell.
    let ready = false;
    for (let i = 0; i < 12 && !ready; i++) {
        if (cancelled())
            return false;
        ready = await daemonReady();
        if (!ready)
            await sleep(500);
    }
    if (!ready) {
        reportEvent('SKIPPED_NO_DAEMON', 'daemon not ready; takeover skipped');
        return false;
    }
    // Bail if the extension was disabled while we were waiting — don't free names
    // after disable() already ran restore().
    if (cancelled()) {
        reportEvent('SKIPPED_CANCELLED', 'disabled during takeover');
        return false;
    }
    await takeoverFdo();
    if (opts.allowGtkTakeover && !cancelled())
        return await takeoverGtk();
    reportEvent('GTK_SKIPPED', 'allowGtkTakeover=false or cancelled');
    return false;
}

/**
 * Restore shell ownership on disable() — no Meta.restart, no override files.
 * The daemon releases the names (Relinquish), the gjs Fdo proxy is reactivated,
 * and the shell re-owns the GTK name (its object path was never unexported).
 */
export async function restore({ gtkWasTakenOver }) {
    await dbusCall(CONTROL_NAME, CONTROL_PATH, CONTROL_NAME, 'Relinquish', null)
        .catch(e => reportEvent('RELINQUISH_FAILED', e.message));

    // Reactivate the gjs proxy so it reclaims org.freedesktop.Notifications.
    await dbusCall(DBUS, DBUS_PATH, DBUS, 'StartServiceByName',
        new GLib.Variant('(su)', [SHELL_NOTIFICATIONS, 0]))
        .catch(e => reportEvent('PROXY_REACTIVATE_FAILED', e.message));

    // Re-own the freed GTK name for the shell (log if the shell fails to reclaim).
    if (gtkWasTakenOver) {
        Gio.DBus.session.own_name(
            GTK_NAME, Gio.BusNameOwnerFlags.REPLACE, null,
            () => logError(new Error('gnome-notistack: shell failed to re-own org.gtk.Notifications')));
    }

    return { needsShellRestart: false };
}
