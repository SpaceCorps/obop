import { useMemo, useRef, useState, type DragEvent, type KeyboardEvent } from "react";
import { inSentence, t } from "@obop/ui";
import type { FieldMeta, Row } from "../api.ts";
import { Cell } from "../fields.tsx";
import { labelField, useData } from "../data.tsx";
import { readPref, writePref } from "../favorites.ts";
import { isEmpty, isMasked, isState, rowCurrency } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import { formatSumRows, groupDomain, groupKey, groupLabel, groupable, sumOf } from "../query.ts";
import { Icon } from "../ui.tsx";
import { actionsOn, canMove, initialOption, isGoverned, moveRecord } from "../workflow.ts";
import type { ViewProps } from "./types.ts";

/** The field the board is grouped by: the person's choice, else a status, else the first groupable. */
export function kanbanField(
  fields: FieldMeta[],
  chosen: string | undefined,
): FieldMeta | undefined {
  const g = groupable(fields);
  return (
    g.find((f) => f.name === chosen) ??
    g.find((f) => isState(f)) ??
    g.find((f) => f.kind === "select") ??
    g.find((f) => f.kind === "ref") ??
    g[0]
  );
}

function defaultsFor(f: FieldMeta, key: string): Record<string, unknown> {
  if (key === "") return {};
  if (f.kind === "bool") return { [f.name]: key === "true" };
  if (f.kind === "date") return { [f.name]: `${key}-01` };
  return { [f.name]: key };
}

export function KanbanView(p: ViewProps) {
  const d = useData();
  const { fields, rows, model } = p;
  const gf = kanbanField(fields, p.state.group[0]);
  const foldKey = `obop-fold:${model.name}:${gf?.name}`;
  const [folded, setFolded] = useState<string[]>(() => readPref<string[]>(foldKey, []));
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [quick, setQuick] = useState<{ key: string; text: string } | null>(null);
  const board = useRef<HTMLDivElement>(null);

  const keys = useMemo(
    () => (gf ? groupDomain(gf, rows, d.lookup) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [gf, rows, d.version],
  );
  if (!gf) return null;

  const titleField = labelField(model);
  const money1 = fields.find((f) => f.kind === "money");
  const secondary = fields
    .filter((f) => f.name !== gf.name && f.name !== titleField?.name)
    .sort((a, b) => weight(a) - weight(b))
    .slice(0, 3);
  // A governed board (D48) moves cards only along declared actions the person may run.
  const governed = isGoverned(model, gf);
  const canDrag =
    gf.kind !== "date" && (governed ? actionsOn(model, gf.name).length > 0 : p.canUpdate);
  const noun = p.title;
  /** Whether the card being dragged (or the given one) may go to column `key`. */
  const may = (id: string | null, key: string) => {
    if (!governed || !id) return { ok: true, reason: "" };
    const row = rows.find((r) => r.id === id);
    if (!row) return { ok: true, reason: "" };
    const m = canMove(model, gf, row[gf.name] as string, key, noun);
    return { ok: m.allowed || m.current, reason: m.reason };
  };

  const setFold = (key: string) => {
    const next = folded.includes(key) ? folded.filter((k) => k !== key) : [...folded, key];
    setFolded(next);
    writePref(foldKey, next);
  };

  async function move(id: string, key: string) {
    if (!gf) return;
    const row = rows.find((r) => r.id === id);
    if (!row || groupKey(gf, row) === key) return;
    if (key === "" && gf.required)
      return d.notify(t("{field} is required", { field: fieldTitle(gf) }), "error");
    if (governed) {
      const m = may(id, key);
      if (!m.ok) return d.notify(m.reason, "error");
      const out = await moveRecord(d.run, model, gf, row, key, noun);
      if (out.kind === "approval") d.notify(out.message, "info");
      else if (out.kind === "error") d.notify(out.message, "error");
      return;
    }
    const value = key === "" ? null : gf.kind === "bool" ? key === "true" : key;
    const err = await p.update(id, { [gf.name]: value });
    if (err) d.notify(err, "error");
  }
  const drop = (e: DragEvent, key: string) => {
    e.preventDefault();
    setOver(null);
    const id = e.dataTransfer.getData("text/plain");
    if (id) void move(id, key);
  };

  async function submitQuick() {
    if (!quick || !gf || !titleField) return;
    const text = quick.text.trim();
    if (!text) return setQuick(null);
    const err = await p.create({ [titleField.name]: text, ...defaultsFor(gf, quick.key) });
    if (err) d.notify(err, "error");
    else setQuick(null);
  }

  const cardKey = (e: KeyboardEvent, row: Row, key: string, index: number) => {
    const focus = (c: number, i: number) => {
      const cards = board.current?.querySelectorAll<HTMLElement>(`[data-col="${c}"] [data-card]`);
      cards?.[Math.min(i, cards.length - 1)]?.focus();
    };
    const col = keys.indexOf(key);
    if (e.key === "Enter") {
      e.preventDefault();
      p.onOpen(row.id);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      focus(col, index + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      focus(col, Math.max(0, index - 1));
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const to = col + (e.key === "ArrowRight" ? 1 : -1);
      if (to < 0 || to >= keys.length) return;
      if (e.shiftKey && canDrag)
        void move(row.id, keys[to]!).then(() => requestAnimationFrame(() => focusById(row.id)));
      else focus(to, index);
    }
  };
  const focusById = (id: string) =>
    board.current?.querySelector<HTMLElement>(`[data-card="${id}"]`)?.focus();

  return (
    <div
      className="kanban"
      ref={board}
      role="group"
      aria-label={t("{model} board by {field}", { model: p.title, field: fieldTitle(gf) })}
    >
      {keys.map((key, col) => {
        const cards = rows.filter((r) => groupKey(gf, r) === key);
        const total = money1 ? sumOf(cards, money1.name) : null;
        const isFolded = folded.includes(key);
        const name = groupLabel(gf, key, d.lookup);
        const lines = secondary;
        return (
          <section
            key={key || "none"}
            data-col={col}
            className={`kcol${over === key ? " over" : ""}${isFolded ? " folded" : ""}${dragging && !may(dragging, key).ok ? " blocked" : ""}`}
            title={dragging && !may(dragging, key).ok ? may(dragging, key).reason : undefined}
            aria-label={t("{name}, {count, plural, one {# card} other {# cards}}", {
              name,
              count: cards.length,
            })}
            onDragOver={(e) => {
              if (!canDrag) return;
              if (!may(dragging, key).ok) {
                e.dataTransfer.dropEffect = "none";
                return;
              }
              e.preventDefault();
              setOver(key);
            }}
            onDragLeave={() => setOver((o) => (o === key ? null : o))}
            onDrop={(e) => canDrag && drop(e, key)}
          >
            <header>
              <button
                className="foldbtn"
                aria-expanded={!isFolded}
                aria-label={isFolded ? t("Unfold {name}", { name }) : t("Fold {name}", { name })}
                onClick={() => setFold(key)}
              >
                <Icon name={isFolded ? "chevron-right" : "chevron-down"} size={13} />
              </button>
              <b className="kname">
                {gf.kind === "select" ? (
                  key ? (
                    <Cell field={gf} value={key} />
                  ) : (
                    <span className="badge t-neutral">{t("None")}</span>
                  )
                ) : (
                  name
                )}
              </b>
              <span className="count">{cards.length}</span>
              {!isFolded && total !== null && total > 0 && (
                <span className="sum" title={t("Total {field}", { field: fieldTitle(money1!) })}>
                  {formatSumRows(money1, cards)}
                </span>
              )}
              {p.canCreate && !isFolded && (!governed || key === initialOption(gf)) && (
                <button
                  className="icon-btn"
                  aria-label={t("Add a card to {name}", { name })}
                  onClick={() => setQuick({ key, text: "" })}
                >
                  <Icon name="plus" size={14} />
                </button>
              )}
            </header>
            {!isFolded && quick?.key === key && (
              <form
                className="quick"
                onSubmit={(e) => {
                  e.preventDefault();
                  void submitQuick();
                }}
              >
                {titleField ? (
                  <input
                    autoFocus
                    aria-label={t("New {field}", { field: inSentence(fieldTitle(titleField)) })}
                    placeholder={t("{field}…", { field: fieldTitle(titleField) })}
                    value={quick.text}
                    onChange={(e) => setQuick({ key, text: e.target.value })}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.stopPropagation();
                        setQuick(null);
                      }
                    }}
                  />
                ) : (
                  <span className="hint">{t("Open the full form to fill this card.")}</span>
                )}
                <div className="actionrow">
                  {titleField && (
                    <button type="submit" className="btn small primary">
                      {t("Add")}
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => {
                      setQuick(null);
                      p.newWith({
                        ...defaultsFor(gf, key),
                        ...(titleField && quick.text.trim()
                          ? { [titleField.name]: quick.text.trim() }
                          : {}),
                      });
                    }}
                  >
                    {t("Edit")}
                  </button>
                </div>
              </form>
            )}
            {!isFolded &&
              cards.map((r, i) => (
                <article
                  key={r.id}
                  data-card={r.id}
                  tabIndex={0}
                  aria-label={
                    titleField
                      ? String(r[titleField.name] ?? "") || r.id.slice(0, 8)
                      : r.id.slice(0, 8)
                  }
                  className={`kcard${r.id === p.selectedId ? " sel" : ""}${dragging === r.id ? " drag" : ""}`}
                  draggable={canDrag}
                  onDragStart={(e) => {
                    e.dataTransfer.setData("text/plain", r.id);
                    e.dataTransfer.effectAllowed = "move";
                    setDragging(r.id);
                  }}
                  onDragEnd={() => setDragging(null)}
                  onClick={() => p.onOpen(r.id)}
                  onKeyDown={(e) => cardKey(e, r, key, i)}
                >
                  <strong className="ktitle">{d.recordLabel(model.name, r)}</strong>
                  {lines.map((f) =>
                    isEmpty(r[f.name]) ? null : (
                      <div key={f.name} className={`kline${f.kind === "money" ? " strong" : ""}`}>
                        <span>{fieldTitle(f)}</span>
                        <span className={isMasked(r[f.name]) ? "" : "kval"}>
                          <Cell
                            field={f}
                            value={r[f.name]}
                            link={false}
                            currency={rowCurrency(r)}
                          />
                        </span>
                      </div>
                    ),
                  )}
                  {/* Keyboard and touch twin of dragging */}
                  {canDrag && (
                    <select
                      className="moveto"
                      aria-label={t("Move {name} to", { name: d.recordLabel(model.name, r) })}
                      value=""
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => e.stopPropagation()}
                      onChange={(e) =>
                        e.target.value !== "" &&
                        void move(r.id, e.target.value === "\u0000" ? "" : e.target.value)
                      }
                    >
                      <option value="">{t("Move to…")}</option>
                      {keys
                        .filter((k) => k !== key && (k !== "" || !gf.required))
                        .map((k) => {
                          const m = governed ? canMove(model, gf, key, k, noun) : null;
                          return (
                            <option
                              key={k}
                              value={k === "" ? "\u0000" : k}
                              disabled={m ? !m.allowed : false}
                              title={m && !m.allowed ? m.reason : undefined}
                            >
                              {groupLabel(gf, k, d.lookup)}
                              {m?.allowed && m.needsApproval ? " *" : ""}
                            </option>
                          );
                        })}
                    </select>
                  )}
                </article>
              ))}
            {!isFolded && cards.length === 0 && quick?.key !== key && (
              <p className="kempty">
                {canDrag ? t("Nothing here. Drop a card.") : t("Nothing here.")}
              </p>
            )}
          </section>
        );
      })}
    </div>
  );
}

function weight(f: FieldMeta): number {
  return {
    image: -1,
    money: 0,
    select: 1,
    date: 2,
    ref: 3,
    number: 4,
    bool: 5,
    text: 6,
    file: 7,
    longtext: 9,
  }[f.kind];
}
