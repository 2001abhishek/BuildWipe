import { useCallback, useReducer } from "react";
import { cancelScan, startScan } from "../lib/api";

const initialState = {
  phase: "idle", // idle | scanning | done
  items: [],
  dirs: 0,
  current: "",
  result: null,
  error: null,
};

function reducer(state, action) {
  switch (action.type) {
    case "start":
      return { ...initialState, phase: "scanning" };
    case "reset":
      return initialState;
    case "found":
      return { ...state, items: state.items.concat(action.items) };
    case "progress":
      return { ...state, dirs: action.dirs, current: action.current };
    case "finished":
      return {
        ...state,
        phase: "done",
        dirs: action.dirs,
        current: "",
        result: {
          cancelled: action.cancelled,
          unreadable: action.unreadable,
          elapsedMs: action.elapsedMs,
        },
      };
    case "error":
      return { ...state, phase: "done", error: action.message };
    case "itemRemoved":
      return { ...state, items: state.items.filter((item) => item.path !== action.path) };
    case "itemShrunk":
      return {
        ...state,
        items: state.items.map((item) =>
          item.path === action.path ? { ...item, size: action.size } : item,
        ),
      };
    default:
      return state;
  }
}

export function useScan() {
  const [state, dispatch] = useReducer(reducer, initialState);

  const start = useCallback((roots) => {
    dispatch({ type: "start" });
    startScan(roots, dispatch).catch((error) =>
      dispatch({ type: "error", message: String(error) }),
    );
  }, []);

  const stop = useCallback(() => {
    cancelScan();
  }, []);

  const reset = useCallback(() => dispatch({ type: "reset" }), []);
  const removeItem = useCallback((path) => dispatch({ type: "itemRemoved", path }), []);
  const shrinkItem = useCallback((path, size) => dispatch({ type: "itemShrunk", path, size }), []);

  return { ...state, start, stop, reset, removeItem, shrinkItem };
}
