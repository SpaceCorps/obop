import { useState } from "react";
import {
  ACCENTS,
  Badge,
  Icon,
  Notice,
  Segmented,
  applyTheme,
  getLang,
  rich,
  t,
  type ThemeSettings,
} from "@obop/ui";
import { href } from "../route.ts";
import { Accessibility } from "./Accessibility.tsx";
import { Components } from "./Components.tsx";
import { Foundations } from "./Foundations.tsx";
import { Iconography } from "./Iconography.tsx";
import { Patterns } from "./Patterns.tsx";
import { Voice } from "./Voice.tsx";
import { useThemeVersion } from "./kit.tsx";

type Notify = (t: string, tone?: "ok" | "error" | "info") => void;

const SECTIONS = [
  {
    id: "foundations",
    label: "Foundations",
    icon: "customize",
    hint: "Logo, colour, type, space, motion",
  },
  { id: "iconography", label: "Iconography", icon: "grid", hint: "Every icon and app tile" },
  { id: "components", label: "Components", icon: "plug", hint: "Every primitive in every state" },
  { id: "patterns", label: "Patterns", icon: "kanban", hint: "Real screens on sample data" },
  { id: "voice", label: "Voice and tone", icon: "mail", hint: "How obop talks" },
  {
    id: "accessibility",
    label: "Accessibility",
    icon: "shield-check",
    hint: "WCAG 2.2 AA in practice",
  },
] as const;
type SectionId = (typeof SECTIONS)[number]["id"];

const ANCHORS: Record<SectionId, [id: string, label: string][]> = {
  foundations: [
    ["logo", "Logo"],
    ["colour", "Colour"],
    ["type", "Typography"],
    ["spacing", "Spacing, radius, elevation"],
    ["motion", "Motion"],
    ["tiles", "App tiles"],
  ],
  iconography: [
    ["icon-rules", "Rules"],
    ["icon-set", "All icons"],
    ["icon-apps", "App icons"],
  ],
  components: [["components", "All components"]],
  patterns: [["patterns", "All patterns"]],
  voice: [
    ["voice", "Principles"],
    ["voice-pairs", "Say it like this"],
    ["voice-words", "Words and formats"],
  ],
  accessibility: [
    ["a11y", "Targets, contrast, focus"],
    ["a11y-keys", "Keyboard map"],
    ["a11y-more", "Motion and assistive tech"],
  ],
};

/** The theme as it is applied right now, read back from the document. */
function liveTheme(): ThemeSettings {
  const root = document.documentElement;
  return {
    accent: getComputedStyle(root).getPropertyValue("--accent").trim() || ACCENTS[0]!.value,
    mode: (root.dataset.theme as ThemeSettings["mode"]) ?? "system",
    density: (root.dataset.density as ThemeSettings["density"]) ?? "comfortable",
    radius: (root.dataset.radius as ThemeSettings["radius"]) ?? "soft",
  };
}

/** Try the theme without leaving the page. Nothing here is saved; Theme saves. */
function ThemeStrip() {
  useThemeVersion();
  const [theme, setT] = useState(liveTheme);
  const set = (patch: Partial<ThemeSettings>) => {
    const next = { ...liveTheme(), ...patch };
    setT(next);
    applyTheme(next);
  };
  return (
    <div
      className="bb-strip"
      role="group"
      aria-label={t("Preview the brand book in another theme")}
    >
      <div className="swatches">
        {ACCENTS.map((a) => (
          <button
            key={a.value}
            className="swatch"
            style={{ background: a.value }}
            aria-label={t(a.name)}
            title={t(a.name)}
            aria-pressed={theme.accent.toLowerCase() === a.value}
            onClick={() => set({ accent: a.value })}
          />
        ))}
      </div>
      <Segmented
        label={t("Brand book mode")}
        value={theme.mode}
        onChange={(mode) => set({ mode })}
        options={[
          { value: "system", label: t("System") },
          { value: "light", label: t("Light") },
          { value: "dark", label: t("Dark") },
        ]}
      />
      <Segmented
        label={t("Brand book density")}
        value={theme.density}
        onChange={(density) => set({ density })}
        options={[
          { value: "comfortable", label: t("Comfortable") },
          { value: "compact", label: t("Compact") },
        ]}
      />
      <Segmented
        label={t("Brand book corners")}
        value={theme.radius}
        onChange={(radius) => set({ radius })}
        options={[
          { value: "sharp", label: t("Sharp") },
          { value: "soft", label: t("Soft") },
          { value: "round", label: t("Round") },
        ]}
      />
    </div>
  );
}

export function BrandBook({
  section,
  goSection,
  notify,
}: {
  section: string;
  goSection: (s: string) => void;
  notify: Notify;
}) {
  const id = (SECTIONS.some((s) => s.id === section) ? section : "foundations") as SectionId;
  const current = SECTIONS.find((s) => s.id === id)!;
  return (
    <div className="bb">
      <header className="bb-intro">
        <div>
          <h2 className="bb-title">{t("Brand book")}</h2>
          <p className="bb-lead">
            {rich(
              "The living style guide. Everything on these pages is rendered with the real components in your current theme, so it is always accurate. The rules are written down in <code>docs/brand/BRAND.md</code>.",
              {},
              { code: (c) => <code>{c}</code> },
            )}
          </p>
        </div>
        <div className="bb-badges">
          <Badge tone="accent">{t("Read-only")}</Badge>
          <Badge tone="info">{t("Follows your theme")}</Badge>
        </div>
      </header>
      <ThemeStrip />
      {getLang() !== "en" && !getLang().startsWith("en-") && (
        <Notice tone="info">
          {t(
            "The Brand book documents the design rules and is written in English. The theme controls above are in your language.",
          )}
        </Notice>
      )}
      <div className="bb-layout" lang="en">
        <nav className="bb-nav" aria-label="Brand book sections">
          <ul>
            {SECTIONS.map((s) => (
              <li key={s.id}>
                <a
                  href={href({ app: "customize", model: "brand", tab: "brand", view: s.id })}
                  aria-current={s.id === id ? "page" : undefined}
                  onClick={(e) => {
                    e.preventDefault();
                    goSection(s.id);
                  }}
                >
                  <Icon name={s.icon} size={16} />
                  <span>{s.label}</span>
                </a>
                {s.id === id && (
                  <ul className="bb-anchors" aria-label={`On this page: ${s.label}`}>
                    {ANCHORS[s.id].map(([a, l]) => (
                      <li key={a}>
                        <button
                          type="button"
                          onClick={() =>
                            document.getElementById(`bb-${a}`)?.scrollIntoView({ block: "start" })
                          }
                        >
                          {l}
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ul>
        </nav>
        <div className="bb-main" aria-label={current.label} role="region">
          {id === "foundations" && <Foundations />}
          {id === "iconography" && <Iconography notify={notify} />}
          {id === "components" && <Components notify={notify} />}
          {id === "patterns" && <Patterns notify={notify} />}
          {id === "voice" && <Voice />}
          {id === "accessibility" && <Accessibility />}
        </div>
      </div>
    </div>
  );
}
