import { useMemo, useState } from "react";
import {
  APP_ICONS,
  AppIcon,
  Icon,
  Segmented,
  UI_ICONS,
  appColor,
  appIconName,
  type IconName,
} from "@obop/ui";
import { Block, DoDont, Specimen, State } from "./kit.tsx";

type Kind = "all" | "ui" | "app";

const label = (n: string) =>
  n
    .split("-")
    .map((w, i) => (i === 0 ? w[0]!.toUpperCase() + w.slice(1) : w))
    .join(" ");

export function Iconography({
  notify,
}: {
  notify: (t: string, tone?: "ok" | "error" | "info") => void;
}) {
  const [q, setQ] = useState("");
  const [kind, setKind] = useState<Kind>("all");
  const names = useMemo(() => {
    const ui = Object.keys(UI_ICONS).map((n) => ({ n, k: "ui" as const }));
    const app = Object.keys(APP_ICONS).map((n) => ({ n, k: "app" as const }));
    const all = [...ui, ...app].filter((i) => kind === "all" || i.k === kind);
    const s = q.trim().toLowerCase();
    return all.filter((i) => !s || i.n.includes(s) || label(i.n).toLowerCase().includes(s));
  }, [q, kind]);

  const copy = async (n: string) => {
    const text = `<Icon name="${n}" />`;
    try {
      await navigator.clipboard.writeText(text);
      notify(`Copied ${text}`);
    } catch {
      notify(`Copy failed. Use ${text}`, "info");
    }
  };

  const apps = Object.keys(APP_ICONS);
  return (
    <>
      <Block
        id="icon-rules"
        title="Iconography"
        lead="One hand-drawn set on a 24px grid: 1.75px stroke, round caps and joins, no fills except small dots. Icons inherit the text colour, so they follow the theme, and they are decorative unless given a label."
      >
        <div className="spec-row">
          <Specimen
            title="The grid"
            note="24px box, 2px padding. Shapes snap to half pixels at 24px; at 16 and 20px the stroke scales with the icon."
          >
            <div className="icon-sizes">
              <State label="16px">
                <Icon name="calendar" size={16} />
              </State>
              <State label="20px (default)">
                <Icon name="calendar" size={20} />
              </State>
              <State label="24px">
                <Icon name="calendar" size={24} />
              </State>
              <State label="48px, grid">
                <span className="icon-grid-demo">
                  <Icon name="calendar" size={96} strokeWidth={1.75} />
                </span>
              </State>
            </div>
          </Specimen>
          <Specimen title="Usage" note="Copy the snippet from any icon below.">
            <pre className="code-sample">
              <code>{`import { Icon, IconButton } from "@obop/ui";\n\n<Icon name="search" />\n<Icon name="shield-check" size={16} label="Verified" />\n<IconButton icon="trash" label="Delete record" />`}</code>
            </pre>
          </Specimen>
          <Specimen title="Rules">
            <DoDont
              dont="An icon with no name as the only content of a button."
              doo="IconButton, which needs a label and shows it as a tooltip."
            />
            <DoDont
              dont="Colour an icon to carry meaning alone (a red trash can)."
              doo="Pair colour with the icon and a word, or use an icon with a state label."
            />
          </Specimen>
        </div>
      </Block>

      <Block
        id="icon-set"
        title="All icons"
        lead={`${Object.keys(UI_ICONS).length} interface icons and ${apps.length} app glyphs. Click one to copy its component.`}
      >
        <div className="icon-tools">
          <div className="searchfield icon-search">
            <Icon name="search" size={16} />
            <input
              type="search"
              placeholder="Search icons"
              aria-label="Search icons"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </div>
          <Segmented
            label="Icon set"
            value={kind}
            onChange={setKind}
            options={[
              { value: "all", label: "All" },
              { value: "ui", label: "Interface" },
              { value: "app", label: "Apps" },
            ]}
          />
          <span className="count" aria-live="polite">
            {names.length} {names.length === 1 ? "icon" : "icons"}
          </span>
        </div>
        <ul className="icon-grid">
          {names.map(({ n, k }) => (
            <li key={n}>
              <button type="button" className="icon-card" onClick={() => void copy(n)}>
                <Icon name={n as IconName} size={24} />
                <span className="icon-name">{n}</span>
                <span className="sr-only">Copy component</span>
                <span className="icon-kind" aria-hidden="true">
                  {k === "app" ? "app" : ""}
                </span>
              </button>
            </li>
          ))}
        </ul>
        {names.length === 0 && <p className="hint">No icon matches "{q}".</p>}
      </Block>

      <Block
        id="icon-apps"
        title="App icons"
        lead="Every app has a glyph and a colour of its own. The tile is generated: gradient from the colour, white glyph, corner 28%. Admins can recolour an app in Customize; the glyph is fixed."
      >
        <ul className="app-icon-grid">
          {apps
            .filter((n) => n !== "app")
            .map((n) => (
              <li key={n}>
                <AppIcon id={n} name={label(n)} size={56} />
                <strong>{label(n)}</strong>
                <code>{appColor(n)}</code>
              </li>
            ))}
          <li>
            <AppIcon id="unknown-module" name="Anything else" size={56} />
            <strong>Other modules</strong>
            <code>{appColor("unknown-module")}</code>
          </li>
        </ul>
        <p className="hint">
          Glyph is picked from the module id (<code>{appIconName("projects")}</code> for projects,
          <code> {appIconName("base")}</code> for base) and falls back to the generic app tile with
          a colour derived from the id.
        </p>
      </Block>
    </>
  );
}
