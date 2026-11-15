// The stock moves a document made (a purchase order's receipts, a manufacturing order's
// production), found by the document number as their source document: how many are done, how many
// wait for a stock manager, and a link to them. A person who may validate gets the button for the
// waiting ones.
import { useEffect, useState } from "react";
import { Badge, fmt, t } from "@obop/ui";
import { search } from "../../api.ts";
import { useData } from "../../data.tsx";
import { encodeState } from "../../query.ts";
import { Button, Icon } from "../../ui.tsx";
import { mayValidate, MOVE, settleMoves } from "./moves.ts";
import "../supply.css";

export function SourceMoves(p: {
  origin: string;
  /** The sentence once every move is done (`{count}` is the number of units moved). */
  doneText: (units: string) => string;
  /** The words of the link (`{count}` is the number of moves). */
  linkText: (count: string) => string;
}) {
  const d = useData();
  const move = d.models.get(MOVE)?.model;
  const [counts, setCounts] = useState<{ done: number; draft: number; units: number } | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!move || !p.origin) return;
    let live = true;
    void search(d.token, MOVE, {
      filter: [{ field: "origin", op: "eq", value: p.origin }],
      limit: 500,
    }).then((r) => {
      if (!live || "error" in r) return;
      const of = (s: string) => r.rows.filter((m) => m.state === s);
      setCounts({
        done: of("done").length,
        draft: of("draft").length,
        units: of("done").reduce((n, m) => n + Number(m.quantity ?? 0), 0),
      });
    });
    return () => {
      live = false;
    };
  }, [d.token, p.origin, d.mutations, move]);
  if (!counts || counts.done + counts.draft === 0) return null;
  const st = encodeState({ facets: [{ op: "is", f: "origin", v: [p.origin] }], group: [] });
  const href = d.linkTo(MOVE, { st });
  async function validate() {
    setBusy(true);
    const { waiting } = await settleMoves(d.run, d.token, move, p.origin);
    await d.changed(MOVE);
    setBusy(false);
    if (waiting === 0) d.notify(t("Stock updated"), "ok");
  }
  return (
    <div className={`sup-receipts${counts.draft ? " sup-waiting" : ""}`} role="note">
      <Icon name="inventory" size={16} />
      <span className="sup-receipts-text">
        {counts.draft === 0
          ? p.doneText(fmt.integer(counts.units))
          : t("{count} stock moves wait for a stock manager", { count: String(counts.draft) })}
      </span>
      {counts.draft > 0 && <Badge tone="warning">{t("To validate")}</Badge>}
      {href && <a href={href}>{p.linkText(String(counts.done + counts.draft))}</a>}
      {counts.draft > 0 && mayValidate(move) && (
        <Button size="sm" loading={busy} onClick={() => void validate()}>
          {t("Validate")}
        </Button>
      )}
    </div>
  );
}
