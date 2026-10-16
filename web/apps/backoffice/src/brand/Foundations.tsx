import { useMemo, useState } from "react";
import {
  APP_ICONS,
  AppIcon,
  Badge,
  Button,
  Icon,
  Logo,
  RAMP_STEPS,
  SIGNALS,
  Table,
  appColor,
  contrast,
} from "@obop/ui";
import {
  Block,
  DoDont,
  Ratio,
  Specimen,
  State,
  tokenContrast,
  tokenHex,
  useThemeVersion,
} from "./kit.tsx";

/* ------------------------------------------------------------------ Logo */

function LogoBlock() {
  return (
    <Block
      id="logo"
      title="Logo"
      lead={
        <>
          A ring with a satellite: one platform, the business in orbit. The wordmark is lowercase,
          geometric and drawn from the same circles, so it needs no font. Both are inline SVG in one
          colour (<code>currentColor</code>).
        </>
      }
    >
      <div className="logo-grid">
        <figure className="logo-tile light">
          <Logo size={44} />
          <figcaption>On light surfaces</figcaption>
        </figure>
        <figure className="logo-tile dark">
          <Logo size={44} />
          <figcaption>On dark surfaces</figcaption>
        </figure>
        <figure className="logo-tile accent">
          <Logo size={44} />
          <figcaption>On the accent</figcaption>
        </figure>
        <figure className="logo-tile mono">
          <Logo size={44} />
          <figcaption>One colour, print or stamp</figcaption>
        </figure>
      </div>

      <div className="spec-row">
        <Specimen
          title="Variants"
          note="Pick by available width. The mark alone is the favicon and the app badge."
        >
          <div className="logo-variants">
            <State label="Full">
              <Logo variant="full" size={32} />
            </State>
            <State label="Mark">
              <Logo variant="mark" size={32} />
            </State>
            <State label="Wordmark">
              <Logo variant="word" size={32} />
            </State>
            <State label="App tile">
              <span className="logo-app-tile" aria-hidden="true">
                <Logo variant="mark" size={24} title="" />
              </span>
            </State>
          </div>
        </Specimen>

        <Specimen
          title="Clear space"
          note={
            <>
              Keep a margin of <b>x</b> on every side, where x is half the height of the logo. No
              text, edge or other mark enters it.
            </>
          }
        >
          <div className="clearspace" style={{ ["--x" as string]: "24px" }}>
            <Logo size={48} />
            <i className="cs-x top">x</i>
            <i className="cs-x left">x</i>
            <i className="cs-x right">x</i>
            <i className="cs-x bottom">x</i>
          </div>
        </Specimen>

        <Specimen
          title="Minimum size"
          note="Below these sizes the rings close up. Use the mark at 16px and up, the full logo from 20px high."
        >
          <div className="logo-min">
            <State label="Mark 16px">
              <Logo variant="mark" size={16} />
            </State>
            <State label="Full 20px">
              <Logo variant="full" size={20} />
            </State>
            <State label="Mark 32px">
              <Logo variant="mark" size={32} />
            </State>
          </div>
        </Specimen>
      </div>

      <h3 className="bb-sub">Name and misuse</h3>
      <div className="misuse">
        <DoDont
          dont={
            <>
              <s>Obop</s>, <s>OBOP</s>, <s>ObOp</s>, <s>the Obop platform</s>
            </>
          }
          doo={
            <>
              <b>obop</b>, always lowercase, even at the start of a sentence: "obop keeps your data
              on one model."
            </>
          }
        />
        <div className="misuse-grid">
          <figure className="misuse-tile">
            <div style={{ transform: "scaleX(1.5)" }}>
              <Logo size={28} />
            </div>
            <figcaption>Do not stretch or squash</figcaption>
          </figure>
          <figure className="misuse-tile">
            <div style={{ transform: "rotate(-12deg)" }}>
              <Logo size={28} />
            </div>
            <figcaption>Do not rotate or tilt</figcaption>
          </figure>
          <figure className="misuse-tile">
            <div
              style={{ filter: "drop-shadow(3px 3px 0 oklch(0.6 0.2 20))", color: "var(--info)" }}
            >
              <Logo size={28} />
            </div>
            <figcaption>Do not add shadows or effects</figcaption>
          </figure>
          <figure className="misuse-tile">
            <div style={{ color: "var(--text-subtle)", opacity: 0.4 }}>
              <Logo size={28} />
            </div>
            <figcaption>Do not place on a busy or low-contrast background</figcaption>
          </figure>
        </div>
      </div>
    </Block>
  );
}

/* ----------------------------------------------------------------- Colour */

function Ramp({ title, prefix, note }: { title: string; prefix: string; note?: string }) {
  const v = useThemeVersion();
  const steps = useMemo(
    () =>
      RAMP_STEPS.map((s) => {
        const hex = tokenHex(`${prefix}-${s}`);
        const onWhite = contrast("#ffffff", hex);
        const onBlack = contrast("#0c0d12", hex);
        return { s, hex, ink: onWhite >= onBlack ? "#ffffff" : "#0c0d12" };
      }),
    // the accent ramp changes with the theme
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [prefix, v],
  );
  return (
    <figure className="ramp">
      <figcaption>
        <b>{title}</b>
        {note && <span className="hint"> {note}</span>}
      </figcaption>
      <ol>
        {steps.map(({ s, hex, ink }) => (
          <li key={s} style={{ background: `var(--${prefix}-${s})`, color: ink }}>
            <b>{s}</b>
            <code>{hex}</code>
          </li>
        ))}
      </ol>
    </figure>
  );
}

const SEMANTIC: [token: string, on: string, role: string, min: number][] = [
  ["text", "surface", "Body text", 4.5],
  ["text-muted", "surface", "Secondary text, labels", 4.5],
  ["text-subtle", "surface", "Placeholders, hints on a canvas", 4.5],
  ["accent-ink", "surface", "Links and active labels", 4.5],
  ["success-ink", "surface", "Success text", 4.5],
  ["warning-ink", "surface", "Warning text", 4.5],
  ["danger-ink", "surface", "Error text", 4.5],
  ["info-ink", "surface", "Information text", 4.5],
  ["border-control", "surface", "Input and control outlines", 3],
  ["accent-fill", "surface", "Buttons and selected controls", 3],
];

function ColourBlock() {
  const v = useThemeVersion();
  const rows = useMemo(
    () =>
      SEMANTIC.map(([t, on, role, min]) => ({
        t,
        on,
        role,
        min,
        hex: tokenHex(t),
        ratio: tokenContrast(t, on),
      })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [v],
  );
  return (
    <Block
      id="colour"
      title="Colour"
      lead="Every colour is an OKLCH ramp with the same lightness steps in every hue, so step 600 means the same thing in violet, green and grey. Components use semantic tokens, never ramp steps directly."
    >
      <div className="ramps">
        <Ramp title="Neutral" prefix="neutral" note="Cool grey. Surfaces, borders and text." />
        <Ramp title="Accent" prefix="accent" note="Generated from the colour picked in Theme." />
        {(Object.keys(SIGNALS) as (keyof typeof SIGNALS)[]).map((k) => (
          <Ramp
            key={k}
            title={k[0]!.toUpperCase() + k.slice(1)}
            prefix={k}
            note={
              k === "success"
                ? "Done, paid, won."
                : k === "warning"
                  ? "Attention, in progress."
                  : k === "danger"
                    ? "Errors, lost, destructive."
                    : "Neutral information."
            }
          />
        ))}
      </div>

      <h3 className="bb-sub">Semantic tokens, measured live</h3>
      <p className="hint">
        These ratios are measured in the browser against the current surface, so they change with
        the accent and the mode. Text needs 4.5:1, outlines and fills 3:1 (WCAG 2.2 AA).
      </p>
      <Table caption="Semantic colour tokens and their contrast on the panel surface">
        <thead>
          <tr>
            <th scope="col">Token</th>
            <th scope="col">Used for</th>
            <th scope="col">Sample</th>
            <th scope="col">Value</th>
            <th scope="col" className="num">
              Contrast
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.t}>
              <th scope="row">
                <code>--{r.t}</code>
              </th>
              <td>{r.role}</td>
              <td>
                <span
                  className="tok-sample"
                  style={{ color: `var(--${r.t})`, background: "var(--surface)" }}
                >
                  {r.t === "border-control" ? (
                    <span className="tok-line" style={{ borderColor: `var(--${r.t})` }} />
                  ) : r.t === "accent-fill" ? (
                    <span className="tok-fill" />
                  ) : (
                    "Aa 123"
                  )}
                </span>
              </td>
              <td>
                <code>{r.hex}</code>
              </td>
              <td className="num">
                <Ratio value={r.ratio} min={r.min} />
              </td>
            </tr>
          ))}
        </tbody>
      </Table>

      <h3 className="bb-sub">Chart palette</h3>
      <p className="hint">
        Eight categorical colours that follow the accent hue. Never rely on colour alone: pair it
        with a label, a pattern or a position.
      </p>
      <ol className="chart-swatches">
        {[1, 2, 3, 4, 5, 6, 7, 8].map((n) => (
          <li key={n}>
            <span style={{ background: `var(--chart-${n})` }} />
            <code>--chart-{n}</code>
          </li>
        ))}
      </ol>
    </Block>
  );
}

/* ------------------------------------------------------------ Typography */

const SCALE: [token: string, px: number, lh: string, weight: number, use: string][] = [
  ["display", 72, "1.05", 600, "Landing hero"],
  ["5xl", 52, "1.08", 600, "Landing section title"],
  ["4xl", 38, "1.1", 600, "Sign-in claim"],
  ["3xl", 30, "1.15", 600, "Home title"],
  ["2xl", 24, "1.15", 600, "Page title"],
  ["xl", 20, "1.3", 600, "Panel and record title"],
  ["lg", 18, "1.3", 600, "Dialog title, section"],
  ["md", 16, "1.5", 400, "Lead paragraph"],
  ["sm", 13, "1.5", 500, "Secondary text, table headers"],
  ["xs", 12, "1.4", 500, "Badges, captions"],
  ["2xs", 11, "1.4", 600, "Overlines (uppercase, tracked)"],
];

function TypeBlock() {
  return (
    <Block
      id="type"
      title="Typography"
      lead={
        <>
          Geist Variable for everything you read, Geist Mono Variable for code and identifiers. Both
          are self-hosted (SIL OFL 1.1), so no request leaves your instance. Geist is precise and
          calm at small sizes, has real tabular figures for money and ships as one variable file per
          script.
        </>
      }
    >
      <div className="spec-row">
        <Specimen title="Geist Variable" note="Sans. Weights 400, 500, 600, 700.">
          <p className="face sans">Aa</p>
          <p className="alphabet">
            ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 €£$ ÅÄÖ ĄĘŁ ЖЯ
          </p>
          <div className="weights">
            {[400, 500, 600, 700].map((w) => (
              <span key={w} style={{ fontWeight: w }}>
                {w} Invoice
              </span>
            ))}
          </div>
        </Specimen>
        <Specimen title="Geist Mono Variable" note="Code, ids, keys, API paths.">
          <p className="face mono">Aa</p>
          <p className="alphabet mono">GET /api/lead/list {'{ stage: "won" }'} 0O 1lI</p>
          <pre className="code-sample">
            <code>{`[[model.field]]\nname  = "driver"\nclass = "internal"`}</code>
          </pre>
        </Specimen>
        <Specimen
          title="Numbers are tabular"
          note="Money and quantities use tabular lining figures, so columns line up and digits never jump."
        >
          <div className="num-compare">
            <div>
              <span className="overline">Proportional</span>
              {["1,111.11", "8,888.88", "1,000.00", "479.05"].map((n) => (
                <p key={n} style={{ fontVariantNumeric: "proportional-nums" }}>
                  {n}
                </p>
              ))}
            </div>
            <div>
              <span className="overline">Tabular (default)</span>
              {["1,111.11", "8,888.88", "1,000.00", "479.05"].map((n) => (
                <p key={n} className="tabular">
                  {n}
                </p>
              ))}
            </div>
          </div>
        </Specimen>
      </div>

      <h3 className="bb-sub">Scale</h3>
      <p className="hint">
        Sizes are rem tokens (16px root). Body is 15px comfortable and 13px compact. Headings use
        weight 600 and tighten tracking as they grow; overlines are uppercase with +0.06em.
      </p>
      <div className="table-wrap">
        <table className="table scale">
          <caption className="sr-only">Type scale</caption>
          <thead>
            <tr>
              <th scope="col">Token</th>
              <th scope="col" className="num">
                Size
              </th>
              <th scope="col" className="num">
                Line height
              </th>
              <th scope="col" className="num">
                Weight
              </th>
              <th scope="col">Sample</th>
            </tr>
          </thead>
          <tbody>
            {SCALE.map(([t, px, lh, w, use]) => (
              <tr key={t}>
                <th scope="row">
                  <code>--text-{t}</code>
                  <span className="hint block">{use}</span>
                </th>
                <td className="num tabular">{px}px</td>
                <td className="num tabular">{lh}</td>
                <td className="num tabular">{w}</td>
                <td>
                  <span
                    className="scale-sample"
                    style={{
                      fontSize: `min(var(--text-${t}), 3.2rem)`,
                      lineHeight: lh,
                      fontWeight: w,
                      letterSpacing: px >= 20 ? "-0.022em" : t === "2xs" ? "0.06em" : undefined,
                      textTransform: t === "2xs" ? "uppercase" : undefined,
                    }}
                  >
                    {px >= 38
                      ? "One platform."
                      : t === "2xs"
                        ? "One business, one platform"
                        : "One business, one platform."}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Block>
  );
}

/* -------------------------------------------------- Space, radius, depth */

const SPACE = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24];

function SpacingBlock() {
  return (
    <Block
      id="spacing"
      title="Spacing, radius and elevation"
      lead="A 4px base. Space tokens are for padding and gaps, never for sizing text. Corners come from one base radius (Sharp 4, Soft 12, Round 20) and every other radius derives from it, so the Corners setting changes the whole product at once."
    >
      <div className="spec-row">
        <Specimen title="Spacing scale">
          <ol className="space-list">
            {SPACE.map((n) => (
              <li key={n}>
                <code>--space-{n}</code>
                <span className="space-bar" style={{ inlineSize: `var(--space-${n})` }} />
                <span className="hint tabular">{n * 4}px</span>
              </li>
            ))}
          </ol>
        </Specimen>
        <Specimen title="Radius scale" note="Shown at the current Corners setting.">
          <ul className="radius-list">
            {[
              ["xs", "Focus rings, keycaps"],
              ["sm", "Chips, menu items"],
              ["md", "Buttons, inputs"],
              ["lg", "Panels, cards"],
              ["xl", "Dialogs, sheets"],
              ["pill", "Badges, switches"],
            ].map(([n, use]) => (
              <li key={n}>
                <span className="radius-box" style={{ borderRadius: `var(--radius-${n})` }} />
                <code>--radius-{n}</code>
                <span className="hint">{use}</span>
              </li>
            ))}
          </ul>
        </Specimen>
        <Specimen
          title="Elevation"
          note="Shadows are soft and cool-tinted; dark mode adds an edge highlight."
        >
          <ul className="elev-list">
            {[
              [1, "Cards, panels, buttons"],
              [2, "Hover lift, raised cards"],
              [3, "Menus, popovers, toasts"],
              [4, "Dialogs and sheets"],
            ].map(([n, use]) => (
              <li key={n}>
                <span className="elev-box" style={{ boxShadow: `var(--shadow-${n})` }}>
                  <code>--shadow-{n}</code>
                </span>
                <span className="hint">{use}</span>
              </li>
            ))}
          </ul>
        </Specimen>
      </div>
    </Block>
  );
}

/* ----------------------------------------------------------------- Motion */

const DURATIONS: [token: string, ms: number, use: string][] = [
  ["dur-1", 90, "Hover and press feedback"],
  ["dur-2", 160, "Menus, tooltips, toggles"],
  ["dur-3", 240, "Dialogs, panels, toasts"],
  ["dur-4", 380, "Page-level transitions"],
];
const EASINGS: [token: string, css: string, p: [number, number, number, number], use: string][] = [
  [
    "ease-out",
    "cubic-bezier(0.16, 1, 0.3, 1)",
    [0.16, 1, 0.3, 1],
    "Things appearing or moving into place",
  ],
  [
    "ease-in-out",
    "cubic-bezier(0.65, 0, 0.35, 1)",
    [0.65, 0, 0.35, 1],
    "Things moving across the screen",
  ],
  ["ease-in", "cubic-bezier(0.5, 0, 0.75, 0)", [0.5, 0, 0.75, 0], "Things leaving the screen"],
];

function Curve({ p }: { p: [number, number, number, number] }) {
  const s = 48;
  const [x1, y1, x2, y2] = p;
  return (
    <svg viewBox="0 0 48 48" width={s} height={s} aria-hidden="true" className="curve">
      <path d="M0 48H48M0 0V48" className="curve-axis" />
      <path d={`M2 46C${2 + x1 * 44} ${46 - y1 * 44} ${2 + x2 * 44} ${46 - y2 * 44} 46 2`} />
    </svg>
  );
}

function MotionBlock() {
  const [run, setRun] = useState(0);
  const reduced =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  return (
    <Block
      id="motion"
      title="Motion"
      lead="Motion explains what changed; it never decorates. Short, eased out, and always optional: with reduced motion on, every duration collapses to near zero and state still changes instantly."
    >
      <div className="spec-row">
        <Specimen title="Durations" note="Press replay to see each one.">
          <div className="motion-list">
            {DURATIONS.map(([t, ms, use]) => (
              <div key={t} className="motion-row">
                <code>--{t}</code>
                <span className="tabular dim">{ms}ms</span>
                <span className="motion-track" aria-hidden="true">
                  <span
                    key={`${t}-${run}`}
                    className={`motion-dot${run > 0 ? " go" : ""}`}
                    style={{
                      animationDuration: `calc(var(--${t}) * 4)`,
                      animationTimingFunction: "var(--ease-out)",
                    }}
                  />
                </span>
                <span className="hint">{use}</span>
              </div>
            ))}
          </div>
          <div className="actionrow">
            <Button icon="replay" onClick={() => setRun((n) => n + 1)}>
              Replay
            </Button>
            {reduced && <Badge tone="info">Reduced motion is on: the dots jump</Badge>}
          </div>
        </Specimen>
        <Specimen title="Easing">
          <div className="easing-list">
            {EASINGS.map(([t, css, p, use]) => (
              <div key={t} className="easing-row">
                <Curve p={p} />
                <div>
                  <code>--{t}</code>
                  <p className="hint">{use}</p>
                  <code className="dim">{css}</code>
                </div>
              </div>
            ))}
          </div>
        </Specimen>
        <Specimen title="Rules">
          <ul className="bb-list">
            <li>Animate transform and opacity only. Never animate layout.</li>
            <li>Nothing moves for longer than 400ms; most things take 160 to 240ms.</li>
            <li>Loading shimmer and spinners stop under reduced motion (a static ring stays).</li>
            <li>No autoplay, parallax or looping decoration anywhere in the product.</li>
          </ul>
        </Specimen>
      </div>
    </Block>
  );
}

/* ----------------------------------------------------------------- Export */

export function Foundations() {
  return (
    <>
      <LogoBlock />
      <ColourBlock />
      <TypeBlock />
      <SpacingBlock />
      <MotionBlock />
      <AppTilesTeaser />
    </>
  );
}

/** The tile recipe lives under Iconography; this is the pointer from the foundations. */
function AppTilesTeaser() {
  const sample = [
    "crm",
    "sales",
    "invoicing",
    "products",
    "contacts",
    "project",
    "helpdesk",
    "fleet",
  ];
  return (
    <Block
      id="tiles"
      title="App tiles"
      lead="Every app is a rounded square with a gradient from its own colour and a white glyph. The tile corner is 28% of its size. Colours are chosen at lightness 0.52 to 0.60 in OKLCH, so the white glyph always reads at 3:1 or better."
    >
      <div className="tile-row">
        {sample.map((id) => (
          <AppIcon key={id} id={id} name={id} color={appColor(id)} size={56} />
        ))}
        <Icon name="arrow-right" size={20} />
        <span className="hint">
          All {Object.keys(APP_ICONS).length - 1} are on the Iconography page.
        </span>
      </div>
    </Block>
  );
}
