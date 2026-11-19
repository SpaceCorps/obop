import { useEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent } from "react";
import { Badge, fmt, getCurrency, type MenuItem } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import { ApprovalMark } from "../ApprovalMark.tsx";
import { blobUrl, thumbWidth } from "../blobs.ts";
import { BlobThumb } from "../BlobFields.tsx";
import { useData } from "../data.tsx";
import { isEmpty, isMasked, optionLabel } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import {
  currencyOf,
  fillTemplate,
  titleOf,
  toneClassOf,
  type PresentCtx,
} from "../presentation.ts";
import { Icon } from "../ui.tsx";
import { shownNumber } from "../inputs/display.ts";
import { displayOf } from "../presentation.ts";
import { dueTone, rangeEndOf, type BoardLayout } from "./board.ts";
import { CardMenu, type CardMenuHandle } from "./CardMenu.tsx";

/** What the presentation functions may ask the data layer: labels and cached rows of other models. */
export function usePresentCtx(moduleId?: string): PresentCtx {
  const d = useData();
  return useMemo(
    () => ({
      refText: d.refText,
      rowOf: (m: string, id: string) => d.store.cached(m)?.find((r) => r.id === id),
      modelOf: (m: string) => d.models.get(m)?.model,
      instanceCurrency: getCurrency(),
      moduleId,
    }),
    // `d` is a new object when labels load or the language changes.
    [d, moduleId],
  );
}

/** The name of a record on a card: the layout's own template, else titleOf. */
export function cardTitle(model: ModelMeta, row: Row, layout: BoardLayout, ctx: PresentCtx) {
  return (
    (layout.titleTemplate && fillTemplate(model, row, layout.titleTemplate, ctx)) ||
    titleOf(model, row, ctx)
  );
}

const localToday = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

/** "14 – 18 Dec", or one date; the year only when it is not this year. */
function dateText(from: unknown, to: unknown, fallback: string): string {
  const a = String(from ?? "").slice(0, 10);
  const b = String(to ?? "").slice(0, 10);
  const day = /^\d{4}-\d{2}-\d{2}$/;
  if (!day.test(a)) return fallback;
  const year = new Date().getFullYear();
  const at = (s: string) => new Date(`${s}T12:00:00`);
  const opts: Intl.DateTimeFormatOptions = {
    day: "numeric",
    month: "short",
    ...(a.slice(0, 4) !== String(year) || (day.test(b) && b.slice(0, 4) !== String(year))
      ? { year: "numeric" }
      : {}),
  };
  try {
    const f = new Intl.DateTimeFormat(fmt.locale, opts);
    if (!day.test(b) || b === a) return f.format(at(a));
    return f.formatRange(at(a), at(b));
  } catch {
    return fallback;
  }
}

/** A stored picture filling the top of a gallery card. */
function Cover({ id, alt }: { id: string; alt: string }) {
  const d = useData();
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  // Fetched once the card comes near the screen, so a long gallery asks only for what it shows.
  const [seen, setSeen] = useState(typeof IntersectionObserver === "undefined");
  useEffect(() => {
    const el = box.current;
    if (seen || !el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setSeen(true);
          io.disconnect();
        }
      },
      { rootMargin: "300px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [seen]);
  useEffect(() => {
    if (!seen) return;
    let live = true;
    const width = box.current?.clientWidth || 280;
    void blobUrl(d.token, id, thumbWidth(width)).then((u) => {
      if (!live) return;
      if (u) setSrc(u);
      else setFailed(true);
    });
    return () => {
      live = false;
    };
  }, [d.token, id, seen]);
  return (
    <div className="kcover" ref={box} data-state={src ? "ready" : failed ? "failed" : "loading"}>
      {src ? (
        <img src={src} alt={alt} loading="lazy" decoding="async" draggable={false} />
      ) : (
        <Icon name={failed ? "warning" : "eye"} size={20} />
      )}
    </div>
  );
}

/** Up to two letters for a record without a picture: "Hanna Vogt" is "HV". */
function initials(name: string): string {
  const words = name.split(/[\s@._/-]+/).filter((w) => /\p{L}/u.test(w));
  return (
    words
      .slice(0, 2)
      .map((w) => [...w].find((c) => /\p{L}/u.test(c))!.toUpperCase())
      .join("") || "?"
  );
}

/** A stable small number from a text, to give each placeholder its own calm tint. */
function hash(s: string): number {
  let h = 0;
  for (const c of s) h = (h * 31 + c.codePointAt(0)!) >>> 0;
  return h;
}

/** A number as a card's figure: "2 h 30", "18.5 km", else "40 Quantity". */
function figureText(f: FieldMeta, v: unknown): string {
  const shown = shownNumber(displayOf(f), v);
  if (shown) return shown;
  const n = Number(v);
  const value = Number.isFinite(n) ? fmt.number(n) : String(v);
  return `${value} ${fieldTitle(f).toLowerCase()}`;
}

/**
 * The facts of a card as short texts in hint order: text as it is, a number with its unit or its
 * field's name ("Quantity 3"), a yes as the field's name, a time of day with the end of its range
 * ("09:00–17:00"). Empty and masked values are left out.
 */
function factsOf(
  names: readonly string[],
  byName: (n: string | undefined) => FieldMeta | undefined,
  row: Row,
  text: (n: string) => string,
): string[] {
  const fields = names.map(byName).filter((f): f is FieldMeta => !!f);
  const out: string[] = [];
  const skip = new Set<string>();
  for (const f of fields) {
    if (skip.has(f.name)) continue;
    const v = row[f.name];
    if (isEmpty(v) || isMasked(v) || v === false) continue;
    const display = displayOf(f);
    if (display === "time_of_day") {
      const end = rangeEndOf(fields, f);
      if (end && !isEmpty(row[end.name])) {
        skip.add(end.name);
        out.push(`${text(f.name)}–${text(end.name)}`);
        continue;
      }
    }
    if ((f.kind === "number" || f.kind === "decimal") && !display) {
      out.push(`${fieldTitle(f)} ${text(f.name)}`);
      continue;
    }
    const s = text(f.name);
    if (s) out.push(s);
  }
  return out;
}

export type NextStep = {
  label: string;
  run: () => void;
  busy: boolean;
  /** The action needs a human's approval when an agent runs it. */
  approval?: boolean;
};

export type BoardCardProps = {
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row;
  layout: BoardLayout;
  ctx: PresentCtx;
  title: string;
  selected: boolean;
  /** Just made with the quick form: highlighted for a moment. */
  fresh?: boolean;
  /** A picture field drawn as the card's cover (the gallery). */
  cover?: FieldMeta;
  /** The record is finished (its column is a terminal state): no date reads as late. */
  finished?: boolean;
  draggable: boolean;
  dragging: boolean;
  /** The ⋯ menu; empty: no menu. */
  menu: MenuItem[];
  menuLabel: string;
  /** The next step of the record's workflow, as a small button (governed boards). */
  next?: NextStep;
  onOpen: () => void;
  onKeyDown?: (e: KeyboardEvent) => void;
  onDragStart?: (e: DragEvent) => void;
  onDragEnd?: () => void;
};

const LONG_PRESS_MS = 500;

/**
 * One record as a card: a cover (gallery), the number above the title, the title, the refs it is
 * about, select values as badges, a progress bar, the date (red once a due date has passed, amber
 * when it is close) and the main amount. A ⋯ button opens its menu (a long press on a touch screen,
 * or the context-menu key); a governed board adds the next step as a small button.
 */
export function BoardCard(p: BoardCardProps) {
  const { model, fields, row, layout, ctx } = p;
  const byName = (n: string | undefined) => (n ? fields.find((f) => f.name === n) : undefined);
  const menu = useRef<CardMenuHandle | null>(null);
  const press = useRef<{ x: number; y: number; timer: number; fired: boolean } | null>(null);

  const text = (n: string) => fillTemplate(model, row, `{${n}}`, ctx) ?? "";
  const masked = (n: string | undefined) => !!n && isMasked(row[n]);

  const subtitle = layout.subtitle
    .map((n) => (masked(n) ? String(row[n]) : text(n)))
    .filter(Boolean);
  const stateF = byName(layout.state);
  const badges = [stateF, ...layout.badges.map(byName)].filter(
    (f): f is FieldMeta => !!f && !isEmpty(row[f.name]) && !isMasked(row[f.name]),
  );
  const moneyF = byName(layout.money);
  const money =
    moneyF && !isEmpty(row[moneyF.name])
      ? isMasked(row[moneyF.name])
        ? String(row[moneyF.name])
        : fmt.money(row[moneyF.name], currencyOf(row, model, ctx, moneyF))
      : "";
  const figureF = byName(layout.figure);
  const figure =
    !money && figureF && !isEmpty(row[figureF.name]) && !isMasked(row[figureF.name])
      ? figureText(figureF, row[figureF.name])
      : "";
  const facts = factsOf(layout.facts, byName, row, text);
  const dateF = byName(layout.date);
  const date =
    dateF && !isEmpty(row[dateF.name]) && !isMasked(row[dateF.name])
      ? dateText(
          row[dateF.name],
          layout.dateEnd ? row[layout.dateEnd] : undefined,
          text(dateF.name),
        )
      : "";
  const late = date
    ? dueTone(
        layout.dateEnd ? byName(layout.dateEnd) : dateF,
        layout.dateEnd ? row[layout.dateEnd] : row[dateF!.name],
        localToday(),
        p.finished,
      )
    : null;
  const overline = layout.overline && !isEmpty(row[layout.overline]) ? text(layout.overline) : "";
  const progressF = byName(layout.progress);
  const progressRaw = progressF ? Number(row[progressF.name]) : NaN;
  const progress = Number.isFinite(progressRaw)
    ? Math.max(0, Math.min(100, progressF?.name.endsWith("_bp") ? progressRaw / 100 : progressRaw))
    : null;
  const avatarF = byName(layout.avatar);
  const coverId = p.cover && !isEmpty(row[p.cover.name]) ? String(row[p.cover.name]) : "";

  const nextButton = p.next ? (
    <button
      type="button"
      className={`btn small knext${date && !money ? " end" : ""}`}
      disabled={p.next.busy}
      data-needs-approval={p.next.approval || undefined}
      onClick={(e) => {
        e.stopPropagation();
        p.next!.run();
      }}
      onKeyDown={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {p.next.label}
      {p.next.approval && <ApprovalMark />}
    </button>
  ) : null;

  const endPress = () => {
    if (press.current) clearTimeout(press.current.timer);
  };

  return (
    <article
      data-card={row.id}
      tabIndex={0}
      aria-label={p.title}
      className={`kcard${p.selected ? " sel" : ""}${p.dragging ? " drag" : ""}${p.fresh ? " fresh" : ""}${p.cover ? " has-cover" : ""}${p.draggable ? " can-drag" : ""}`}
      draggable={p.draggable}
      onDragStart={p.onDragStart}
      onDragEnd={p.onDragEnd}
      onClick={() => {
        // The click that ends a long press opened the menu; it does not open the record too.
        if (press.current?.fired) {
          press.current = null;
          return;
        }
        p.onOpen();
      }}
      onKeyDown={(e) => {
        if ((e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) && p.menu.length) {
          e.preventDefault();
          menu.current?.open();
          return;
        }
        p.onKeyDown?.(e);
      }}
      onContextMenu={(e) => {
        if (!p.menu.length || press.current?.fired) return;
        // A long press on a touch screen also fires this; the timer below opens the menu.
        if (press.current) {
          e.preventDefault();
          return;
        }
        e.preventDefault();
        menu.current?.open();
      }}
      onPointerDown={(e) => {
        if (e.pointerType !== "touch" || !p.menu.length) return;
        const timer = window.setTimeout(() => {
          if (!press.current) return;
          press.current.fired = true;
          navigator.vibrate?.(10);
          menu.current?.open();
        }, LONG_PRESS_MS);
        press.current = { x: e.clientX, y: e.clientY, timer, fired: false };
      }}
      onPointerMove={(e) => {
        const at = press.current;
        if (at && !at.fired && Math.hypot(e.clientX - at.x, e.clientY - at.y) > 10) {
          endPress();
          press.current = null;
        }
      }}
      onPointerUp={endPress}
      onPointerCancel={() => {
        endPress();
        press.current = null;
      }}
    >
      {coverId && <Cover id={coverId} alt={p.title} />}
      {p.cover && !coverId && (
        <div className={`kcover empty t-${(hash(p.title) % 6) + 1}`} aria-hidden="true">
          <span className="kinitials">{initials(p.title)}</span>
        </div>
      )}
      <div className="khead">
        {avatarF && !isEmpty(row[avatarF.name]) && !p.cover && (
          <BlobThumb id={String(row[avatarF.name])} alt="" size={32} className="kavatar" />
        )}
        <div className="kname-block">
          {overline && <span className="kover">{overline}</span>}
          <strong className="ktitle">{p.title}</strong>
          {subtitle.length > 0 && <span className="ksub">{subtitle.join(" · ")}</span>}
          {facts.length > 0 && <span className="kfacts">{facts.join(" · ")}</span>}
        </div>
        {p.menu.length > 0 && <CardMenu label={p.menuLabel} items={p.menu} handle={menu} />}
      </div>
      {badges.length > 0 && (
        <div className="kbadges">
          {badges.map((f) => (
            <Badge key={f.name} tone={toneClassOf(f, String(row[f.name]), model) as `t-${string}`}>
              {optionLabel(String(row[f.name]), f)}
            </Badge>
          ))}
        </div>
      )}
      {progress !== null && (
        <div className="kprogress-row">
          <div
            className="kprogress"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(progress)}
            aria-label={progressF ? fieldTitle(progressF) : undefined}
          >
            <span style={{ inlineSize: `${progress}%` }} />
          </div>
          <span className="kprogress-value">{fmt.percent(progress / 100, 0)}</span>
        </div>
      )}
      {(date || money || figure || p.next) && (
        <div className="kfoot">
          {date && (
            <span className={`kdate${late ? ` ${late}` : ""}`}>
              <Icon name={late === "bad" ? "warning" : "calendar"} size={13} />
              {date}
            </span>
          )}
          {/* The next step shares the line with the date or the amount; with both it gets its own. */}
          {nextButton && !(date && (money || figure)) && nextButton}
          {money && <span className="kmoney amount">{money}</span>}
          {figure && (
            <span className="kmoney amount kfigure" title={fieldTitle(figureF!)}>
              {figure}
            </span>
          )}
        </div>
      )}
      {nextButton && date && (money || figure) && nextButton}
    </article>
  );
}
