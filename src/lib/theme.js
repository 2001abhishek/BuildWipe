import { getCurrentWindow } from "@tauri-apps/api/window";

const STORAGE_KEY = "buildwipe.theme";
export const THEME_PREFERENCES = ["dark", "light", "system"];

const systemQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

export function readPreference() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return THEME_PREFERENCES.includes(saved) ? saved : "system";
  } catch {
    return "system";
  }
}

export function savePreference(preference) {
  try {
    localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // Without storage the choice simply isn't remembered.
  }
}

/** The palette to show: "dark" or "light". */
export function resolveTheme(preference) {
  if (preference !== "system") return preference;
  return systemQuery().matches ? "dark" : "light";
}

export function paintTheme(theme) {
  document.documentElement.dataset.theme = theme;
}

/** Keeps the native title bar in step. Null hands it back to the OS setting. */
export function setWindowTheme(preference) {
  try {
    getCurrentWindow()
      .setTheme(preference === "system" ? null : preference)
      .catch((error) => console.warn("Could not set the window theme:", error));
  } catch {
    // Not running inside Tauri, e.g. `npm run dev` in a browser.
  }
}

export function watchSystemTheme(onChange) {
  const query = systemQuery();
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}
