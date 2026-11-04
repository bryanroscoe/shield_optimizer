import {
  invoke
} from "./chunk-YQTFE5VL.js";
import "./chunk-UGBVNEQM.js";

// node_modules/@tauri-apps/plugin-process/dist-js/index.js
async function exit(code = 0) {
  await invoke("plugin:process|exit", { code });
}
async function relaunch() {
  await invoke("plugin:process|restart");
}
export {
  exit,
  relaunch
};
//# sourceMappingURL=@tauri-apps_plugin-process.js.map
