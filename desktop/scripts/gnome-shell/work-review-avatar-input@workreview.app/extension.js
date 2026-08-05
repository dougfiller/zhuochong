import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_PATH = '/org/gnome/shell/extensions/WorkReviewAvatarInput';
const DBUS_XML = `
<node>
  <interface name="org.gnome.shell.extensions.WorkReviewAvatarInput">
    <method name="GetPointer">
      <arg type="s" name="payload" direction="out"/>
    </method>
    <method name="GetInput">
      <arg type="s" name="payload" direction="out"/>
    </method>
  </interface>
</node>`;

function mouseGroupFromModifiers(modifiers) {
  if (modifiers & Clutter.ModifierType.BUTTON1_MASK)
    return 'mouse-left';

  if (modifiers & Clutter.ModifierType.BUTTON3_MASK)
    return 'mouse-right';

  if (modifiers & Clutter.ModifierType.BUTTON2_MASK)
    return 'mouse-side';

  return 'mouse-move';
}

class WorkReviewAvatarInputService {
  constructor() {
    this._lastKeyval = 0;
    this._lastKeycode = 0;
    this._lastKeyboardTimestampMs = 0;
  }

  recordKeyboardEvent(event) {
    const keyval = typeof event.get_key_symbol === 'function' ? event.get_key_symbol() : 0;
    const keycode = typeof event.get_key_code === 'function' ? event.get_key_code() : 0;

    if (!keyval)
      return;

    this._lastKeyval = keyval;
    this._lastKeycode = keycode;
    this._lastKeyboardTimestampMs = Math.floor(GLib.get_monotonic_time() / 1000);
  }

  _buildPayload() {
    const [x, y, modifiers] = global.get_pointer();

    return JSON.stringify({
      x: Math.round(x),
      y: Math.round(y),
      mouseGroup: mouseGroupFromModifiers(modifiers),
      keyval: this._lastKeyval,
      keycode: this._lastKeycode,
      keyboardTimestampMs: this._lastKeyboardTimestampMs,
      timestampMs: Math.floor(GLib.get_monotonic_time() / 1000),
    });
  }

  GetPointer() {
    return this._buildPayload();
  }

  GetInput() {
    return this._buildPayload();
  }
}

export default class WorkReviewAvatarInputExtension extends Extension {
  enable() {
    if (this._dbusObject)
      return;

    this._service = new WorkReviewAvatarInputService();
    this._dbusObject = Gio.DBusExportedObject.wrapJSObject(DBUS_XML, this._service);
    this._dbusObject.export(Gio.DBus.session, DBUS_PATH);
    this._capturedEventId = global.stage.connect('captured-event', (_actor, event) => {
      const eventType = typeof event?.type === 'function'
        ? event.type()
        : event?.get_type?.();

      if (eventType === Clutter.EventType.KEY_PRESS)
        this._service.recordKeyboardEvent(event);

      return Clutter.EVENT_PROPAGATE;
    });
  }

  disable() {
    if (this._capturedEventId) {
      global.stage.disconnect(this._capturedEventId);
      this._capturedEventId = 0;
    }

    this._dbusObject?.unexport();
    this._dbusObject = null;
    this._service = null;
  }
}
