import { useEffect, useState, type ReactNode } from "react";
import { Badge, Icon, Table, contrast, toHex, type IconName } from "@obop/ui";

/** Re-renders when the theme (accent, mode, density, corners or the system preference) changes. */
export function useThemeVersion(): number {
  const [v, setV] = useState(0);
  useEffect(() => {
    const bump = () => setV((n) => n + 1);
    const mo = new MutationObserver(bump);
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["style", "data-theme", "data-density", "data-radius"],
    });
    const mq = matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", bump);
    return () => {
      mo.disconnect();
      mq.removeEventListener("change", bump);
    };
  }, []);
  return v;
}

let probeCtx: CanvasRenderingContext2D | null | undefined;

/** The sRGB hex a CSS colour (oklch, color-mix, var()) really resolves to in this browser. */
export function resolveColor(css: string): string {
  const probe = document.createElement("span");
  probe.style.color = css;
  probe.style.display = "none";
  document.body.appendChild(probe);
  const resolved = getComputedStyle(probe).color;
  probe.remove();
  if (probeCtx === undefined) {
    const c = document.createElement("canvas");
    c.width = c.height = 1;
    probeCtx = c.getContext("2d", { willReadFrequently: true });
  }
  if (!probeCtx) return "#000000";
  probeCtx.clearRect(0, 0, 1, 1);
  probeCtx.fillStyle = "#000";
  probeCtx.fillStyle = resolved;
  probeCtx.fillRect(0, 0, 1, 1);
  const [r, g, b] = probeCtx.getImageData(0, 0, 1, 1).data;
  return toHex([r! / 255, g! / 255, b! / 255]);
}

export const tokenHex = (name: string) => resolveColor(`var(--${name})`);

/** Contrast of a token against a surface token, as it renders right now. */
export function tokenContrast(fg: string, bg: string): number {
  return contrast(tokenHex(fg), tokenHex(bg));
}

export function Ratio({ value, min }: { value: number; min: number }) {
  const ok = value >= min;
  return (
    <span className={`ratio ${ok ? "pass" : "fail"}`}>
      <Icon name={ok ? "check" : "close"} size={13} />
      <span className="tabular">{value.toFixed(1)}:1</span>
      <span className="sr-only">{ok ? "passes" : "fails"}</span>
    </span>
  );
}

/** A named block of the book: heading, one-line purpose and its content. */
export function Block({
  id,
  title,
  lead,
  children,
}: {
  id: string;
  title: string;
  lead?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="bb-block" id={`bb-${id}`} aria-labelledby={`bb-${id}-h`}>
      <header className="bb-block-head">
        <h2 id={`bb-${id}-h`}>{title}</h2>
        {lead && <p className="bb-lead">{lead}</p>}
      </header>
      {children}
    </section>
  );
}

/** One component or pattern on show: demo area plus an optional props table. */
export function Specimen({
  title,
  note,
  children,
  props,
  wide,
}: {
  title: string;
  note?: ReactNode;
  children: ReactNode;
  props?: [name: string, type: string, def: string, desc: string][];
  wide?: boolean;
}) {
  return (
    <article className={`spec${wide ? " wide" : ""}`}>
      <header className="spec-head">
        <h3>{title}</h3>
        {note && <p className="hint">{note}</p>}
      </header>
      <div className="spec-demo">{children}</div>
      {props && (
        <details className="spec-props">
          <summary>Props</summary>
          <Table dense caption={`${title} props`}>
            <thead>
              <tr>
                <th scope="col">Prop</th>
                <th scope="col">Type</th>
                <th scope="col">Default</th>
                <th scope="col">What it does</th>
              </tr>
            </thead>
            <tbody>
              {props.map(([n, t, d, c]) => (
                <tr key={n}>
                  <th scope="row">
                    <code>{n}</code>
                  </th>
                  <td>
                    <code>{t}</code>
                  </td>
                  <td>
                    <code>{d}</code>
                  </td>
                  <td>{c}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </details>
      )}
    </article>
  );
}

/** A labelled state in a grid of states (default, hover, focus, disabled...). */
export function State({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="state">
      <span className="state-label">{label}</span>
      <div className="state-demo">{children}</div>
    </div>
  );
}

export function DoDont({ dont, doo, why }: { dont: ReactNode; doo: ReactNode; why?: ReactNode }) {
  return (
    <div className="dodont">
      <div className="dont">
        <Badge tone="danger">Don't</Badge>
        <p>{dont}</p>
      </div>
      <div className="do">
        <Badge tone="success">Do</Badge>
        <p>{doo}</p>
      </div>
      {why && <p className="hint dodont-why">{why}</p>}
    </div>
  );
}

export function Pill({ icon, children }: { icon: IconName; children: ReactNode }) {
  return (
    <span className="bb-pill">
      <Icon name={icon} size={14} /> {children}
    </span>
  );
}
