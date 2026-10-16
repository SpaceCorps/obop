import { useMemo } from "react";
import { Badge, Icon, Notice, SURFACES, fmt as loc, resolveAccent, t, type Check } from "@obop/ui";

const fmt = (n: number) =>
  `${loc.number(n, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}:1`;

function Cell({ c }: { c: Check }) {
  return (
    <td className="num">
      <span className={`ratio ${c.pass ? "pass" : "fail"}`}>
        <Icon name={c.pass ? "check" : "close"} size={13} />
        <span className="tabular">{fmt(c.ratio)}</span>
        <span className="sr-only">
          {c.pass ? t("passes") : t("fails")}, {t("needs {min}:1", { min: c.min })}
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
        <h4>{t("Contrast check")}</h4>
        <Badge tone={all ? "success" : "danger"}>
          {all ? t("WCAG 2.2 AA passes") : t("Fails AA")}
        </Badge>
      </div>
      {a.adjusted && (
        <Notice tone="info" title={t("Adjusted to stay readable")}>
          {t(
            "Your colour {chosen} is too light or too dark for text. Buttons, links and selected states use {light} in light mode and {dark} in dark mode so they pass AA.",
            { chosen: a.chosen, light: a.light.fill, dark: a.dark.fill },
          )}
        </Notice>
      )}
      {!a.adjusted && a.darkShifted && (
        <p className="hint">
          {t(
            "Your colour passes as chosen. Dark mode uses a lighter step of the same hue ({fill}) so it stands out on dark surfaces.",
            { fill: a.dark.fill },
          )}
        </p>
      )}
      <div className="table-wrap">
        <table className="table dense">
          <caption className="sr-only">{t("Contrast ratios of the accent colour")}</caption>
          <thead>
            <tr>
              <th scope="col">{t("Pair")}</th>
              <th scope="col" className="num">
                {t("Light")}
              </th>
              <th scope="col" className="num">
                {t("Dark")}
              </th>
              <th scope="col" className="num">
                {t("Needs")}
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ light, dark }) => (
              <tr key={light.id}>
                <th scope="row">{t(light.label)}</th>
                <Cell c={light} />
                <Cell c={dark} />
                <td className="num dim tabular">{fmt(light.min)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="contrast-sw" aria-hidden="true">
        <span style={{ background: a.light.fill, color: a.light.onFill }}>{t("Aa light")}</span>
        <span style={{ background: a.dark.fill, color: a.dark.onFill }}>{t("Aa dark")}</span>
        <span style={{ background: SURFACES.light.panel, color: a.light.ink }}>
          {t("Link light")}
        </span>
        <span style={{ background: SURFACES.dark.panel, color: a.dark.ink }}>{t("Link dark")}</span>
      </div>
    </div>
  );
}
