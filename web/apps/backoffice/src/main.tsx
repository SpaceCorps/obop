import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { I18nProvider, initI18n } from "@obop/ui";
import "@obop/ui/styles.css";
import { App } from "./App.tsx";
import { ErrorBoundary } from "./ErrorBoundary.tsx";
import "./style.css";
import "./controlpanel.css";
import "./views.css";
import "./record.css";
import "./blobs.css";
import "./customize.css";
import "./modules/modules.css";
import "./updates/updates.css";
import "./dashboards/dashboards.css";
import "./connectors/connectors.css";
import "./guide.css";
import "./brand/brand.css";

// One lazy chunk per language and package: only the active language is downloaded, English is inline.
const loaders = [
  import.meta.glob("../../../packages/ui/locales/*/messages.json", { import: "default" }),
  import.meta.glob("../locales/*/messages.json", { import: "default" }),
];

// Errors that no screen catches are reported to the console only: nothing leaves the browser.
addEventListener("unhandledrejection", (e) => {
  console.error("[obop] unhandled promise rejection", e.reason);
  e.preventDefault();
});

void initI18n({ loaders }).then(() =>
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ErrorBoundary scope="app">
        <I18nProvider>
          <App />
        </I18nProvider>
      </ErrorBoundary>
    </StrictMode>,
  ),
);
