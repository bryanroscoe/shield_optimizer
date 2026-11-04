import {
  Image
} from "./chunk-CMUBLXJB.js";
import {
  addPluginListener,
  invoke
} from "./chunk-YQTFE5VL.js";
import "./chunk-UGBVNEQM.js";

// node_modules/@tauri-apps/api/app.js
var BundleType;
(function(BundleType2) {
  BundleType2["Nsis"] = "nsis";
  BundleType2["Msi"] = "msi";
  BundleType2["Deb"] = "deb";
  BundleType2["Rpm"] = "rpm";
  BundleType2["AppImage"] = "appimage";
  BundleType2["App"] = "app";
})(BundleType || (BundleType = {}));
async function getVersion() {
  return invoke("plugin:app|version");
}
async function getName() {
  return invoke("plugin:app|name");
}
async function getTauriVersion() {
  return invoke("plugin:app|tauri_version");
}
async function getIdentifier() {
  return invoke("plugin:app|identifier");
}
async function show() {
  return invoke("plugin:app|app_show");
}
async function hide() {
  return invoke("plugin:app|app_hide");
}
async function fetchDataStoreIdentifiers() {
  return invoke("plugin:app|fetch_data_store_identifiers");
}
async function removeDataStore(uuid) {
  return invoke("plugin:app|remove_data_store", { uuid });
}
async function defaultWindowIcon() {
  return invoke("plugin:app|default_window_icon").then((rid) => rid ? new Image(rid) : null);
}
async function setTheme(theme) {
  return invoke("plugin:app|set_app_theme", { theme });
}
async function setDockVisibility(visible) {
  return invoke("plugin:app|set_dock_visibility", { visible });
}
async function getBundleType() {
  return invoke("plugin:app|bundle_type");
}
async function onBackButtonPress(handler) {
  return addPluginListener("app", "back-button", handler);
}
async function supportsMultipleWindows() {
  return invoke("plugin:app|supports_multiple_windows");
}
export {
  BundleType,
  defaultWindowIcon,
  fetchDataStoreIdentifiers,
  getBundleType,
  getIdentifier,
  getName,
  getTauriVersion,
  getVersion,
  hide,
  onBackButtonPress,
  removeDataStore,
  setDockVisibility,
  setTheme,
  show,
  supportsMultipleWindows
};
//# sourceMappingURL=@tauri-apps_api_app.js.map
