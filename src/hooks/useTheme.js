import { useEffect, useState } from "react";
import {
  paintTheme,
  readPreference,
  resolveTheme,
  savePreference,
  setWindowTheme,
  watchSystemTheme,
} from "../lib/theme";

/** The user's theme choice: "dark", "light" or "system". */
export function useTheme() {
  const [preference, setPreference] = useState(readPreference);

  useEffect(() => {
    savePreference(preference);
    setWindowTheme(preference);
    const repaint = () => paintTheme(resolveTheme(preference));
    repaint();
    return preference === "system" ? watchSystemTheme(repaint) : undefined;
  }, [preference]);

  return [preference, setPreference];
}
