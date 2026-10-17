import { Component, useEffect, useRef, type ErrorInfo, type ReactNode } from "react";
import { t } from "@obop/ui";
import { useData } from "./data.tsx";
import { probe, useNet } from "./net.ts";
import { Button, EmptyState, Icon } from "./ui.tsx";

/**
 * The friendly recovery screen for a screen that crashed. The error goes to the console only: the
 * app never sends error reports anywhere. `resetKey` clears the error when the person navigates.
 */
export class ErrorBoundary extends Component<
  { children: ReactNode; resetKey?: unknown; scope?: "app" | "page" },
  { error: Error | null }
> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("[obop] a screen failed to render", error, info.componentStack);
  }

  componentDidUpdate(prev: { resetKey?: unknown }) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const page = this.props.scope === "page";
    return (
      <div className={page ? "recovery page" : "recovery app"} role="alert" data-testid="recovery">
        <EmptyState
          icon="warning"
          title={t("Something went wrong on this screen")}
          hint={t(
            "Nothing was lost or changed by this error. Try again, or reload the page. If it keeps happening, tell whoever runs this system what you were doing.",
          )}
          action={{ label: t("Try again"), onClick: () => this.setState({ error: null }) }}
          secondary={{ label: t("Reload the page"), onClick: () => location.reload() }}
        />
        <p className="recovery-links">
          <a href="#/">{t("Go to your apps")}</a>
        </p>
        <details className="recovery-details">
          <summary>{t("Technical details")}</summary>
          <pre>{`${error.name}: ${error.message}`}</pre>
        </details>
      </div>
    );
  }
}

function Banner({ inline }: { inline?: boolean }) {
  const net = useNet();
  if (!net.down) return null;
  return (
    <div className={`offline-banner${inline ? " inline" : ""}`} role="alert">
      <Icon name="warning" size={16} />
      <span>
        {net.offline
          ? t("You are offline. Changes cannot be saved until the connection is back.")
          : t("The server cannot be reached. Changes cannot be saved until it is back.")}
      </span>
      <Button size="sm" onClick={() => void probe()}>
        {t("Try again")}
      </Button>
    </div>
  );
}

/** The banner on the screen that shows before any data has loaded. */
export function OfflineNotice() {
  return <Banner inline />;
}

/**
 * The page-wide banner for a lost connection. It clears itself when the server answers again and
 * then asks for everything on screen once more.
 */
export function OfflineBanner() {
  const { down } = useNet();
  const d = useData();
  const was = useRef(false);
  const refresh = useRef(d.refreshAll);
  refresh.current = d.refreshAll;

  useEffect(() => {
    if (was.current && !down) refresh.current();
    was.current = down;
  }, [down]);
  useEffect(() => {
    if (!down) return;
    const timer = setInterval(() => void probe(), 10000);
    return () => clearInterval(timer);
  }, [down]);

  return <Banner />;
}
