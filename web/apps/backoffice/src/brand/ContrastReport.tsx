import { useMemo } from "react";
import { Badge, Icon, Notice, SURFACES, fmt as loc, resolveAccent, t, type Check } from "@obop/ui";

const fmt = (n: number) =>
  `${loc.number(n, { minimumFractionDigits: 1, maximumFractionDigits: 1 })}:1`;

/** `label` is the column name, shown above the ratio when the table stacks on a phone. */
function Cell({ c, label }: { c: Check; label: string }) {
  return (
    <td role="cell" className="num" data-label={label}>
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
        <table className="table dense stack-sm" role="table">
          <caption className="sr-only">{t("Contrast ratios of the accent colour")}</caption>
          <thead role="rowgroup">
            <tr role="row">
              <th scope="col" role="columnheader">
                {t("Pair")}
              </th>
              <th scope="col" role="columnheader" className="num">
                {t("Light")}
              </th>
              <th scope="col" role="columnheader" className="num">
                {t("Dark")}
              </th>
              <th scope="col" role="columnheader" className="num">
                {t("Needs")}
              </th>
            </tr>
          </thead>
          <tbody role="rowgroup">
            {rows.map(({ light, dark }) => (
              <tr role="row" key={light.id}>
                <th scope="row" role="rowheader">
                  {t(light.label)}
                </th>
                <Cell c={light} label={t("Light")} />
                <Cell c={dark} label={t("Dark")} />
                <td role="cell" className="num dim tabular" data-label={t("Needs")}>
                  {fmt(light.min)}
                </td>
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
