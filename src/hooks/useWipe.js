import { useCallback, useState } from "react";
import { cancelWipe, startWipe } from "../lib/api";

/** Runs a wipe and reports each outcome back as it happens. */
export function useWipe({ onRemoved, onShrunk, onFinished }) {
  const [wipe, setWipe] = useState(null);

  const start = useCallback(
    (paths) => {
      setWipe({ phase: "running", total: paths.length, done: 0, freed: 0, failures: [], cancelled: false });

      const handle = (event) => {
        switch (event.type) {
          case "removed":
            onRemoved(event.path);
            setWipe((w) => ({ ...w, done: w.done + 1, freed: w.freed + event.freed }));
            break;
          case "failed":
            if (event.freed > 0) onShrunk(event.path, event.remaining);
            setWipe((w) => ({
              ...w,
              done: w.done + 1,
              freed: w.freed + event.freed,
              failures: [...w.failures, { path: event.path, error: event.error }],
            }));
            break;
          case "finished":
            setWipe((w) => ({ ...w, phase: "finished", cancelled: event.cancelled }));
            onFinished();
            break;
        }
      };

      startWipe(paths, handle).catch((error) => {
        setWipe((w) => ({
          ...w,
          phase: "finished",
          failures: [{ path: "BuildWipe", error: String(error) }],
        }));
      });
    },
    [onRemoved, onShrunk, onFinished],
  );

  const stop = useCallback(() => {
    cancelWipe();
  }, []);

  const dismiss = useCallback(() => setWipe(null), []);

  return { wipe, start, stop, dismiss };
}
