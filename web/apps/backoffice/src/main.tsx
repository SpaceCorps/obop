import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@obop/ui/styles.css";
import { App } from "./App.tsx";
import "./style.css";
import "./controlpanel.css";
import "./views.css";
import "./record.css";
import "./customize.css";
import "./dashboards/dashboards.css";
import "./guide.css";
import "./brand/brand.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
