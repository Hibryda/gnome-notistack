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
        for (const src of this._sources.values())
            src.destroy();
        this._sources.clear();
    }

    _source(appName, appIcon) {
        const existing = this._sources.get(appName);
        if (existing)
            return existing;
        const params = { title: appName || 'Notifications' };
        if (appIcon) {
            if (appIcon.startsWith('/') || appIcon.startsWith('file://'))
                params.icon = Gio.icon_new_for_string(appIcon);
            else
                params.iconName = appIcon;
        }
        const source = new MessageTray.Source(params);
        source.connect('destroy', () => this._sources.delete(appName));
        Main.messageTray.add(source);
        this._sources.set(appName, source);
        return source;
    }

    _post(appName, appIcon, summary, body) {
        const source = this._source(appName, appIcon);
        const notification = new MessageTray.Notification({
            source,
            title: summary || appName || 'Notification',
            body: body || '',
            isTransient: false,
        });
        notification.acknowledged = true; // list it, don't re-banner
        source.addNotification(notification);
    }
}
