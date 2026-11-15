// "Stock by location" on a product, when Inventory is installed: where its units are, summed on
// the server over the product's stock items (`inventory_item.search` with `aggregate`), with a
// link to its stock list. Units at a scrap location are not stock: they are left out of the places
// and shown apart as scrapped.
import { useEffect, useState } from "react";
import { fmt, t } from "@obop/ui";
import { aggregate } from "../../api.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { useData, useRows } from "../../data.tsx";
import { encodeState } from "../../query.ts";
import { Icon } from "../../ui.tsx";
import "../supply.css";

const ITEM = "inventory_item";

export function OnHand({ row }: RecordExtProps) {
  const d = useData();
  const has = d.models.has(ITEM);
  const { rows: locations } = useRows("inventory_location", has);
  const [stock, setStock] = useState<{ places: [string, number][] } | null>(null);
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
      setStock({ places });
    });
    return () => {
      live = false;
    };
  }, [d.token, row.id, has, d.mutations]);
  if (!has || !stock) return null;
  const scrapAt = new Set(locations.filter((l) => l.kind === "scrap").map((l) => l.id));
  const places = stock.places.filter(([loc]) => !scrapAt.has(loc));
  const scrapped = stock.places.filter(([loc]) => scrapAt.has(loc)).reduce((n, [, q]) => n + q, 0);
  const total = places.reduce((n, [, q]) => n + q, 0);
  const st = encodeState({ facets: [{ op: "is", f: "product_id", v: [row.id] }], group: [] });
  const href = d.linkTo(ITEM, { st });
  const shown = places.slice(0, 5);
  return (
    <section className="sup-onhand" aria-label={t("Stock by location")}>
      <div className="sup-onhand-head">
        <Icon name="inventory" size={18} />
        <span className="sup-onhand-label">{t("Stock by location")}</span>
        {places.length > 0 && (
          <span className="sup-onhand-total num">
            {t("{count} on hand", { count: fmt.integer(total) })}
          </span>
        )}
        {href && (
          <a href={href} className="sup-onhand-link">
            {t("All stock")}
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
          {places.length > shown.length && (
            <li className="hint">
              {t("and {count} more locations", {
                count: String(places.length - shown.length),
              })}
            </li>
          )}
        </ul>
      ) : (
        <p className="hint">{t("Not in stock anywhere.")}</p>
      )}
      {scrapped !== 0 && (
        <p className="hint sup-onhand-scrap">
          {t("{count} scrapped, not counted as stock", { count: fmt.integer(scrapped) })}
        </p>
      )}
    </section>
  );
}
