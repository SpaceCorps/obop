// "On hand" on a product, when Inventory is installed: the units in stock, summed on the server
// over the product's stock items (`inventory_item.search` with `aggregate`), with the locations
// that hold it and a link to its stock list.
import { useEffect, useState } from "react";
import { fmt, t } from "@obop/ui";
import { aggregate } from "../../api.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { useData } from "../../data.tsx";
import { encodeState } from "../../query.ts";
import { Icon } from "../../ui.tsx";
import "../supply.css";

const ITEM = "inventory_item";

export function OnHand({ row }: RecordExtProps) {
  const d = useData();
  const has = d.models.has(ITEM);
  const [stock, setStock] = useState<{ units: number; places: [string, number][] } | null>(null);
  useEffect(() => {
    if (!has) return;
    let live = true;
    void aggregate(d.token, ITEM, {
      filter: [{ field: "product_id", op: "eq", value: row.id }],
      group_by: [{ field: "location_id" }],
      measures: [{ op: "sum", field: "quantity" }],
    }).then((a) => {
      if (!live || "error" in a) return;
      const places = a.groups
        .map((g) => [String(g.key.location_id ?? ""), Number(g.measures["sum:quantity"] ?? 0)])
        .filter(([, n]) => n !== 0) as [string, number][];
      places.sort((x, y) => y[1] - x[1]);
      setStock({ units: places.reduce((n, [, q]) => n + q, 0), places });
    });
    return () => {
      live = false;
    };
  }, [d.token, row.id, has, d.mutations]);
  if (!has || !stock) return null;
  const st = encodeState({ facets: [{ op: "is", f: "product_id", v: [row.id] }], group: [] });
  const href = d.linkTo(ITEM, { st });
  const shown = stock.places.slice(0, 3);
  return (
    <section className="sup-onhand" aria-label={t("On hand")}>
      <div className="sup-onhand-head">
        <Icon name="inventory" size={18} />
        <span className="sup-onhand-label">{t("On hand")}</span>
        <strong className="sup-onhand-n">{fmt.integer(stock.units)}</strong>
        {href && (
          <a href={href} className="sup-onhand-link">
            {t("Stock")}
            <Icon name="chevron-right" size={14} />
          </a>
        )}
      </div>
      {shown.length > 0 ? (
        <ul className="sup-onhand-places">
          {shown.map(([loc, n]) => (
            <li key={loc}>
              <span>{d.refText("inventory_location", loc)}</span>
              <span className="num">{fmt.integer(n)}</span>
            </li>
          ))}
          {stock.places.length > shown.length && (
            <li className="hint">
              {t("and {count} more locations", {
                count: String(stock.places.length - shown.length),
              })}
            </li>
          )}
        </ul>
      ) : (
        <p className="hint">{t("Not in stock anywhere.")}</p>
      )}
    </section>
  );
}
