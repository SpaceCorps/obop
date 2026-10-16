import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@obop/ui/styles.css";
import { App } from "./App.tsx";
import "./style.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
