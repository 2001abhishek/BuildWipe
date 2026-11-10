import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export const appVersion = () => getVersion();

/** The newer release on GitHub, or null when this is the latest version. */
export const findUpdate = () => check();

/**
 * Downloads and installs `update`, reporting bytes as they arrive, then
 * restarts into the new version. On Windows the installer closes the app itself.
 */
export async function installUpdate(update, onProgress) {
  let total = 0;
  let received = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
      onProgress({ received, total });
    } else if (event.event === "Finished") {
      onProgress({ received: total || received, total, installing: true });
    }
  });
  await relaunch();
}
