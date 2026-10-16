import { useMemo } from "react";
import { Badge, Icon, Notice, SURFACES, resolveAccent, type Check } from "@obop/ui";

const fmt = (n: number) => `${n.toFixed(1)}:1`;

function Cell({ c }: { c: Check }) {
  return (
    <td className="num">
      <span className={`ratio ${c.pass ? "pass" : "fail"}`}>
        <Icon name={c.pass ? "check" : "close"} size={13} />
        <span className="tabular">{fmt(c.ratio)}</span>
        <span className="sr-only">
          {c.pass ? "passes" : "fails"}, needs {c.min}:1
        </span>
      </span>
    </td>
  );
}

/**
 * The automatic WCAG 2.2 AA check for an accent colour, in light and dark: the ratios the theme
 * picker measured, and a note when it had to lighten or darken the colour to pass.
 */
export function ContrastReport({ accent }: { accent: string }) {
  const a = useMemo(() => resolveAccent(accent), [accent]);
  const rows = a.light.checks.map((c, i) => ({ light: c, dark: a.dark.checks[i]! }));
  const all = [...a.light.checks, ...a.dark.checks].every((c) => c.pass);
  return (
    <div className="contrast">
      <div className="contrast-head">
        <h4>Contrast check</h4>
        <Badge tone={all ? "success" : "danger"}>{all ? "WCAG 2.2 AA passes" : "Fails AA"}</Badge>
      </div>
      {a.adjusted && (
        <Notice tone="info" title="Adjusted to stay readable">
          Your colour {a.chosen} is too light or too dark for text. Buttons, links and selected
          states use {a.light.fill} in light mode and {a.dark.fill} in dark mode so they pass AA.
        </Notice>
      )}
      {!a.adjusted && a.darkShifted && (
        <p className="hint">
          Your colour passes as chosen. Dark mode uses a lighter step of the same hue ({a.dark.fill}
          ) so it stands out on dark surfaces.
        </p>
      )}
      <div className="table-wrap">
        <table className="table dense">
          <caption className="sr-only">Contrast ratios of the accent colour</caption>
          <thead>
            <tr>
              <th scope="col">Pair</th>
              <th scope="col" className="num">
                Light
              </th>
              <th scope="col" className="num">
                Dark
              </th>
              <th scope="col" className="num">
                Needs
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ light, dark }) => (
              <tr key={light.id}>
                <th scope="row">{light.label}</th>
                <Cell c={light} />
                <Cell c={dark} />
                <td className="num dim tabular">{fmt(light.min)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="contrast-sw" aria-hidden="true">
        <span style={{ background: a.light.fill, color: a.light.onFill }}>Aa light</span>
        <span style={{ background: a.dark.fill, color: a.dark.onFill }}>Aa dark</span>
        <span style={{ background: SURFACES.light.panel, color: a.light.ink }}>Link light</span>
        <span style={{ background: SURFACES.dark.panel, color: a.dark.ink }}>Link dark</span>
      </div>
    </div>
  );
}
