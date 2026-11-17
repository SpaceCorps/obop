import { useId } from "react";
import { t } from "@obop/ui";
import { PHONE_QUERY, TOUCH_QUERY, useMedia } from "../media.ts";
import { StatusChip } from "./badges.tsx";
import { dependencyStatus, graphOf, prettyReq } from "./status.ts";
import type { Row } from "./types.ts";

const COL = 220;
const W = 176;
// A box of the picture is a link: a finger needs 44 px of it, a mouse the compact 34.
const BOX = { mouse: { row: 50, h: 34 }, touch: { row: 56, h: 44 } };
const PAD = 12;

/**
 * Every installed module and what it needs, as a picture (built-in modules on the left, each
 * package one column right of what it needs) and, for anyone who cannot use a picture, the same
 * facts as a table. Both are there on a wider screen; the table is the accessible name of the
 * picture. A phone shows only the table, open and one block per module: the picture is wider
 * than the screen and its boxes were cut off.
 */
export function DependencyOverview({
  rows,
  nameOf,
  hrefFor,
}: {
  rows: Row[];
  nameOf: (id: string) => string;
  hrefFor: (id: string) => string;
}) {
  const id = useId();
  const phone = useMedia(PHONE_QUERY);
  const { row: ROW, h: H } = useMedia(TOUCH_QUERY) ? BOX.touch : BOX.mouse;
  const g = graphOf(rows);
  const maxRow = Math.max(0, ...g.nodes.map((n) => n.row));
  const width = PAD * 2 + (g.layers - 1) * COL + W;
  const height = PAD * 2 + maxRow * ROW + H;
  const at = new Map(g.nodes.map((n) => [n.id, n]));
  const pos = (n: { layer: number; row: number }) => ({
    x: PAD + n.layer * COL,
    y: PAD + n.row * ROW,
  });
  const needy = rows.filter((r) => r.dependencies.length > 0 || r.dependents.length > 0);
  // stack-sm turns each row into a block on a phone (data-label names the cells there). The
  // explicit roles keep it a table for Safari, which drops the semantics once display changes.
  const table = (
    <div className="table-wrap">
      <table className="table stack-sm" role="table">
        <caption className="sr-only">{t("Needs and required by, per installed module")}</caption>
        <thead>
          <tr role="row">
            <th scope="col" role="columnheader">
              {t("Module")}
            </th>
            <th scope="col" role="columnheader">
              {t("Needs")}
            </th>
            <th scope="col" role="columnheader">
              {t("Required by")}
            </th>
          </tr>
        </thead>
        <tbody>
          {needy.map((r) => (
            <tr key={r.id} role="row">
              <th scope="row" role="rowheader">
                <a href={hrefFor(r.id)}>{nameOf(r.id)}</a>
              </th>
              <td role="cell" data-label={t("Needs")}>
                {r.dependencies.length === 0 ? (
                  <span className="hint">{t("Nothing")}</span>
                ) : (
                  <ul className="plain">
                    {r.dependencies.map((d) => (
                      <li key={d.id}>
                        {nameOf(d.id)} <code>{prettyReq(d.requirement)}</code>{" "}
                        <StatusChip status={dependencyStatus(d)} />
                      </li>
                    ))}
                  </ul>
                )}
              </td>
              <td role="cell" data-label={t("Required by")}>
                {r.dependents.length === 0 ? (
                  <span className="hint">{t("Nothing")}</span>
                ) : (
                  r.dependents.map(nameOf).join(", ")
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
  if (phone)
    return (
      <div className="dep-overview">
        <div className="dep-table-wrap">{table}</div>
      </div>
    );
  return (
    <div className="dep-overview">
      <div className="dep-svg-wrap">
        <svg
          viewBox={`0 0 ${width} ${height}`}
          width={width}
          height={height}
          role="img"
          aria-labelledby={`${id}-t ${id}-d`}
          className="dep-svg"
        >
          <title id={`${id}-t`}>{t("Dependencies between the installed modules")}</title>
          <desc id={`${id}-d`}>
            {t(
              "An arrow runs from a module to each module it needs. The table below lists the same.",
            )}
          </desc>
          <defs>
            <marker
              id={`${id}-a`}
              viewBox="0 0 8 8"
              refX="7"
              refY="4"
              markerWidth="7"
              markerHeight="7"
              orient="auto"
            >
              <path d="M0 0 L8 4 L0 8 z" className="dep-arrow" />
            </marker>
          </defs>
          {g.edges.map((e) => {
            const from = at.get(e.from);
            const to = at.get(e.to);
            if (!from || !to) return null;
            const a = pos(from);
            const b = pos(to);
            // From the left edge of the dependent to the right edge of what it needs.
            const x1 = a.x;
            const y1 = a.y + H / 2;
            const x2 = b.x + W;
            const y2 = b.y + H / 2;
            const mid = (x1 + x2) / 2;
            return (
              <path
                key={`${e.from}-${e.to}`}
                d={`M${x1} ${y1} C${mid} ${y1} ${mid} ${y2} ${x2 + 1} ${y2}`}
                className={e.satisfied ? "dep-edge" : "dep-edge bad"}
                markerEnd={`url(#${id}-a)`}
              />
            );
          })}
          {g.nodes.map((n) => {
            const p = pos(n);
            return (
              <a
                key={n.id}
                href={hrefFor(n.id)}
                className={n.builtin ? "dep-box builtin" : "dep-box"}
              >
                <rect x={p.x} y={p.y} width={W} height={H} rx={8} />
                <text x={p.x + 12} y={p.y + H / 2} dominantBaseline="central">
                  {clip(nameOf(n.id), 22)}
                </text>
              </a>
            );
          })}
        </svg>
      </div>
      <details className="dep-table-wrap">
        <summary>{t("Show the same as a table")}</summary>
        {table}
      </details>
    </div>
  );
}

function clip(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}
