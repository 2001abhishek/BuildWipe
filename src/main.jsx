import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/manrope";
import "@fontsource-variable/jetbrains-mono";
import "./index.css";
import App from "./App";
import { paintTheme, readPreference, resolveTheme } from "./lib/theme";

// Paint the saved theme before the first render, so the app never flashes the wrong one.
paintTheme(resolveTheme(readPreference()));

ReactDOM.createRoot(document.getElementById("root")).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
