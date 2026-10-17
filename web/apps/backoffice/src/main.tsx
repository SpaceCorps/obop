import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { I18nProvider, initI18n } from "@obop/ui";
import "@obop/ui/styles.css";
import { App } from "./App.tsx";
import "./style.css";
import "./controlpanel.css";
import "./views.css";
import "./record.css";
import "./customize.css";
import "./dashboards/dashboards.css";
import "./connectors/connectors.css";
import "./guide.css";
import "./brand/brand.css";

// One lazy chunk per language and package: only the active language is downloaded, English is inline.
const loaders = [
  import.meta.glob("../../../packages/ui/locales/*/messages.json", { import: "default" }),
  import.meta.glob("../locales/*/messages.json", { import: "default" }),
];

void initI18n({ loaders }).then(() =>
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <I18nProvider>
        <App />
      </I18nProvider>
    </StrictMode>,
  ),
);
