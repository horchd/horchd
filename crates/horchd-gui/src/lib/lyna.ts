// Detection + open helpers for Lyna, the companion trainer/studio.
// Lyna is a separate web app that lives at http://localhost:5173 by
// default. horchd-gui never embeds Lyna; it just opens it in the user's
// default browser when they click "Train new wakeword in Lyna…".

import { openUrl } from "@tauri-apps/plugin-opener";

const LYNA_URL = "http://localhost:5173";
const LYNA_GITHUB = "https://github.com/horchd/lyna";

/** Probe whether Lyna's dev/prod server is reachable. */
export async function lynaReachable(): Promise<boolean> {
  try {
    const ctrl = new AbortController();
    const t = setTimeout(() => ctrl.abort(), 1500);
    await fetch(LYNA_URL, { method: "HEAD", signal: ctrl.signal, mode: "no-cors" });
    clearTimeout(t);
    return true;
  } catch {
    return false;
  }
}

/** Open Lyna locally if reachable, otherwise the GitHub install page. */
export async function openLyna(): Promise<"local" | "github"> {
  const local = await lynaReachable();
  await openUrl(local ? LYNA_URL : LYNA_GITHUB);
  return local ? "local" : "github";
}

export const LYNA_ENDPOINTS = { LYNA_URL, LYNA_GITHUB } as const;
