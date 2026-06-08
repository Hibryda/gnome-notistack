// mirror.js — push notistack popups into GNOME's notification list (date menu).
//
// The daemon shows the popup AND emits a `Posted` signal on its control
// interface; here we add each to Main.messageTray with `acknowledged = true`, so
// it appears in the date-menu list WITHOUT a second banner (the daemon already
// shows one). Notifications group by app, like native GNOME.

import Gio from 'gi://Gio';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as MessageTray from 'resource:///org/gnome/shell/ui/messageTray.js';

const CONTROL_NAME = 'store.hemoglobina.notistack.Control';
const CONTROL_PATH = '/store/hemoglobina/notistack/Control';

export class Mirror {
    constructor() {
        this._subId = 0;
        this._sources = new Map(); // app_name -> MessageTray.Source
    }

    enable() {
        this._subId = Gio.DBus.session.signal_subscribe(
            null, CONTROL_NAME, 'Posted', CONTROL_PATH, null,
            Gio.DBusSignalFlags.NONE,
            (_conn, _sender, _path, _iface, _signal, params) => {
                try {
                    const [appName, appIcon, summary, body] = params.deepUnpack();
                    this._post(appName, appIcon, summary, body);
                } catch (e) {
                    logError(e, 'gnome-notistack: mirror post failed');
                }
            });
    }

    disable() {
        if (this._subId) {
            Gio.DBus.session.signal_unsubscribe(this._subId);
            this._subId = 0;
        }
        // Snapshot first: src.destroy() fires the 'destroy' handler synchronously,
        // which mutates this._sources — don't mutate while iterating it.
        const sources = [...this._sources.values()];
        this._sources.clear();
        for (const src of sources)
            src.destroy();
    }

    _source(appName) {
        const existing = this._sources.get(appName);
        if (existing)
            return existing;
        // Icon comes from untrusted notification content. Only allow a themed
        // name (no attacker file:// / absolute path handed to the shell's icon
        // loader). The daemon's own popup decodes the real image for display.
        const source = new MessageTray.Source({
            title: appName || 'Notifications',
            iconName: 'dialog-information-symbolic',
        });
        source.connect('destroy', () => this._sources.delete(appName));
        Main.messageTray.add(source);
        this._sources.set(appName, source);
        return source;
    }

    _post(appName, _appIcon, summary, body) {
        const source = this._source(appName);
        const notification = new MessageTray.Notification({
            source,
            title: summary || appName || 'Notification',
            body: body || '',
            // Pin the safe defaults explicitly: body is plain text (never parse
            // attacker markup in the shell process), and the entry just lists
            // (the daemon already showed the banner-equivalent popup).
            useBodyMarkup: false,
            isTransient: false,
        });
        notification.acknowledged = true; // list it, don't re-banner
        source.addNotification(notification);
    }
}
