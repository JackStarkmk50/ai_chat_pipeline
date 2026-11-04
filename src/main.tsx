
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// Screenshot/testing hook: ?theme=dark forces the dark palette.
if (new URLSearchParams(window.location.search).get("theme") === "dark") {
  document.documentElement.classList.add("rv-dark");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

