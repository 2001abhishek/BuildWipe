import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

function channelFor(onEvent) {
  const channel = new Channel();
  channel.onmessage = onEvent;
  return channel;
}

export const listDisks = () => invoke("list_disks");
export const startScan = (roots, onEvent) =>
  invoke("start_scan", { roots, onEvent: channelFor(onEvent) });
export const cancelScan = () => invoke("cancel_scan");
export const clearScan = () => invoke("clear_scan");
export const startWipe = (paths, onEvent) =>
  invoke("start_wipe", { paths, onEvent: channelFor(onEvent) });
export const cancelWipe = () => invoke("cancel_wipe");
export const pickFolder = () => open({ directory: true, multiple: false });
export const reveal = (path) => revealItemInDir(path);
