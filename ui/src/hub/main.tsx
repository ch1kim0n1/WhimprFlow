import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { THEME_CSS, applyStoredTheme } from "./theme";

applyStoredTheme();

const style = document.createElement("style");
style.textContent = THEME_CSS + `html, body, #root { margin: 0; height: 100%; } * { box-sizing: border-box; } body { background: var(--wf-pageBg); }`;
document.head.appendChild(style);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
