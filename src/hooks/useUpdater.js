import { useCallback, useEffect, useRef, useState } from "react";
import { appVersion, findUpdate, installUpdate } from "../lib/updater";

/**
 * Update state for the header button and dialog. Checks quietly once on
 * launch, so a new version shows up without anyone asking, and again
 * whenever the user clicks the button.
 *
 * status: idle | checking | current | available | installing | failed
 */
export function useUpdater() {
  const [version, setVersion] = useState("");
  const [state, setState] = useState({ status: "idle", update: null, progress: null, error: null });
  const launchCheckDone = useRef(false);

  const checkNow = useCallback(async ({ quiet = false } = {}) => {
    setState((s) => ({ ...s, status: "checking", error: null }));
    try {
      const update = await findUpdate();
      setState({ status: update ? "available" : "current", update, progress: null, error: null });
    } catch (error) {
      // A quiet launch check fails silently, for example when offline.
      setState({ status: quiet ? "idle" : "failed", update: null, progress: null, error: String(error) });
    }
  }, []);

  useEffect(() => {
    appVersion().then(setVersion).catch(() => {});
    if (launchCheckDone.current) return;
    launchCheckDone.current = true;
    checkNow({ quiet: true });
  }, [checkNow]);

  const install = useCallback(async () => {
    const { update } = state;
    if (!update) return;
    setState((s) => ({ ...s, status: "installing", progress: { received: 0, total: 0 }, error: null }));
    try {
      await installUpdate(update, (progress) => setState((s) => ({ ...s, progress })));
    } catch (error) {
      setState((s) => ({ ...s, status: "available", progress: null, error: String(error) }));
    }
  }, [state]);

  // "Up to date" and "Couldn't check" are brief confirmations, then the button returns to normal.
  useEffect(() => {
    if (state.status !== "current" && state.status !== "failed") return;
    const timer = setTimeout(() => setState((s) => ({ ...s, status: "idle" })), 4000);
    return () => clearTimeout(timer);
  }, [state.status]);

  return { ...state, version, checkNow, install };
}
