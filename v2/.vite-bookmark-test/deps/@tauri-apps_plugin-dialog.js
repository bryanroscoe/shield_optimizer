import {
  invoke
} from "./chunk-YQTFE5VL.js";
import "./chunk-UGBVNEQM.js";

// node_modules/@tauri-apps/plugin-dialog/dist-js/index.js
function buttonsToRust(buttons) {
  if (buttons === void 0) {
    return void 0;
  }
  if (typeof buttons === "string") {
    return buttons;
  } else if ("ok" in buttons && "cancel" in buttons) {
    return { OkCancelCustom: [buttons.ok, buttons.cancel] };
  } else if ("yes" in buttons && "no" in buttons && "cancel" in buttons) {
    return {
      YesNoCancelCustom: [buttons.yes, buttons.no, buttons.cancel]
    };
  } else if ("ok" in buttons) {
    return { OkCustom: buttons.ok };
  }
  return void 0;
}
async function open(options = {}) {
  if (typeof options === "object") {
    Object.freeze(options);
  }
  return await invoke("plugin:dialog|open", { options });
}
async function save(options = {}) {
  if (typeof options === "object") {
    Object.freeze(options);
  }
  return await invoke("plugin:dialog|save", { options });
}
async function messageCommand(message2, options) {
  return await invoke("plugin:dialog|message", {
    message: message2,
    title: options == null ? void 0 : options.title,
    kind: options == null ? void 0 : options.kind,
    buttons: buttonsToRust(options == null ? void 0 : options.buttons)
  });
}
async function message(message2, options) {
  const opts = typeof options === "string" ? { title: options } : options;
  if (opts && !opts.buttons && opts.okLabel) {
    opts.buttons = { ok: opts.okLabel };
  }
  return messageCommand(message2, opts);
}
async function ask(message2, options) {
  const opts = typeof options === "string" ? { title: options } : options;
  const customButtons = (opts == null ? void 0 : opts.okLabel) || (opts == null ? void 0 : opts.cancelLabel);
  const okLabel = (opts == null ? void 0 : opts.okLabel) ?? "Yes";
  return await messageCommand(message2, {
    title: opts == null ? void 0 : opts.title,
    kind: opts == null ? void 0 : opts.kind,
    buttons: customButtons ? { ok: okLabel, cancel: opts.cancelLabel ?? "No" } : "YesNo"
  }) === okLabel;
}
async function confirm(message2, options) {
  const opts = typeof options === "string" ? { title: options } : options;
  const customButtons = (opts == null ? void 0 : opts.okLabel) || (opts == null ? void 0 : opts.cancelLabel);
  const okLabel = (opts == null ? void 0 : opts.okLabel) ?? "Ok";
  return await messageCommand(message2, {
    title: opts == null ? void 0 : opts.title,
    kind: opts == null ? void 0 : opts.kind,
    buttons: customButtons ? { ok: okLabel, cancel: opts.cancelLabel ?? "Cancel" } : "OkCancel"
  }) === okLabel;
}
export {
  ask,
  confirm,
  message,
  open,
  save
};
//# sourceMappingURL=@tauri-apps_plugin-dialog.js.map
