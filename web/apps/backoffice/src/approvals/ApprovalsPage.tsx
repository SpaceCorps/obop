// The Approvals page (D97): the requests agents made that wait for a person, and the decided ones.
// A row only opens its plan: no Approve or Reject on a row, a badge or a toast (rule 10). One plan
// is on screen at a time, in PlanPanel, with its change visible and a button that names the act.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Badge, Button, EmptyState, Skeleton, Table, Tabs, panelId, t } from "@obop/ui";
import { useData } from "../data.tsx";
import type { Go, Route } from "../route.ts";
import { planList, type PlanRow } from "./planApi.ts";
import { PlanPanel } from "./PlanPanel.tsx";
import { APPROVALS, tabOf, type ApprovalsTab } from "./route.ts";
import { plansChanged } from "./usePlanCount.ts";
import { namesFrom, statusWord, thingOf, waitingLine, whenOf, whoOf, type Names } from "./words.ts";
import "./approvals.css";

type List =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "ok"; rows: PlanRow[]; total: number; more?: "busy" | "failed" };

export function ApprovalsPage({ route, go }: { route: Route; go: Go }) {
  const d = useData();
  const tab = tabOf(route);
  const open = route.id;
  // The labels are read again in a new language (d.lang).
  const names = useMemo(() => namesFrom(d.models), [d.models, d.lang]);
  const [list, setList] = useState<List>({ kind: "loading" });
  const heading = useRef<HTMLHeadingElement>(null);

  // `quiet`: read again behind the open panel, without the list going back to a skeleton.
  const load = useCallback(
    async (quiet = false) => {
      if (!quiet) setList({ kind: "loading" });
      const r = await planList(d.token, tab);
      // The bar's count reads again whenever the list was read: a plan may have arrived since.
      // (The count's own read never reads the list, so this does not loop.)
      plansChanged();
      setList((was) =>
        r.ok
          ? { kind: "ok", rows: r.value.rows, total: r.value.total }
          : quiet && was.kind === "ok"
            ? was
            : { kind: "error" },
      );
    },
    [d.token, tab],
  );
  /** The next fifty, below the ones shown (the badge counts all of them). */
  const more = useCallback(async () => {
    if (list.kind !== "ok") return;
    const have = list.rows;
    setList({ ...list, more: "busy" });
    const r = await planList(d.token, tab, have.length);
    plansChanged();
    setList((was) => {
      if (was.kind !== "ok") return was;
      if (!r.ok) return { ...was, more: "failed" };
      const seen = new Set(have.map((x) => x.id));
      return {
        kind: "ok",
        rows: [...have, ...r.value.rows.filter((x) => !seen.has(x.id))],
        total: r.value.total,
      };
    });
  }, [d.token, tab, list]);
  useEffect(() => {
    void load();
  }, [load]);

  const put = (patch: Partial<Route>, replace = false) =>
    go(
      {
        app: APPROVALS,
        ...(tab === "decided" ? { view: "decided" } : {}),
        ...patch,
      },
      { replace },
    );

  // Focus goes back to the row that opened the plan when it closes. After a decision it goes to
  // the heading: the row is leaving the list, and the button that opened the dialog is gone, so
  // the kit's dialogs have nothing to give focus back to (it would be `body`). This effect depends
  // on `open` only: the list is read again right after a decision, and an effect that re-ran when
  // the list changed would cancel the frame that was waiting to move focus.
  const lastOpen = useRef(open);
  const decided = useRef(false);
  useEffect(() => {
    const was = lastOpen.current;
    lastOpen.current = open;
    if (!was || open) return;
    const afterDecision = decided.current;
    decided.current = false;
    const move = () => {
      const row = afterDecision
        ? null
        : document.querySelector<HTMLElement>(`[data-plan="${CSS.escape(was)}"]`);
      (row ?? heading.current)?.focus({ preventScroll: false });
    };
    const frame = requestAnimationFrame(move);
    // A dialog that was open gives focus back once more after 300 ms when nothing has it; by then
    // the heading has it, but if that gave up, take it here rather than leave it on `body`.
    const late = afterDecision
      ? setTimeout(() => {
          const a = document.activeElement;
          if (!a || a === document.body) move();
        }, 350)
      : undefined;
    return () => {
      cancelAnimationFrame(frame);
      if (late) clearTimeout(late);
    };
  }, [open]);

  const tabs: { id: ApprovalsTab; label: string }[] = [
    { id: "waiting", label: t("Waiting") },
    { id: "decided", label: t("Decided") },
  ];

  return (
    <>
      <section className="main panel appr-page" aria-labelledby="appr-title">
        <header className="appr-head">
          <h1 id="appr-title" ref={heading} tabIndex={-1}>
            {t("Approvals")}
          </h1>
        </header>
        <Tabs
          label={t("Approvals")}
          idPrefix="appr"
          tabs={tabs}
          value={tab}
          onChange={(v) => go({ app: APPROVALS, ...(v === "decided" ? { view: "decided" } : {}) })}
        />
        <div
          className="content appr-content"
          role="tabpanel"
          id={panelId("appr")}
          aria-labelledby={`appr-tab-${tab}`}
          aria-busy={list.kind === "loading" || undefined}
        >
          {list.kind === "loading" ? (
            <Skeleton rows={3} cols={3} />
          ) : list.kind === "error" ? (
            <EmptyState
              icon="warning"
              title={t("The approvals could not be loaded.")}
              action={{ label: t("Try again"), onClick: () => void load() }}
            />
          ) : list.rows.length === 0 ? (
            tab === "waiting" ? (
              <EmptyState
                icon="check"
                title={t("Nothing is waiting for you.")}
                hint={t("When an agent proposes a change that needs a person, it appears here.")}
              />
            ) : (
              <EmptyState
                icon="check"
                title={t("Nothing was decided in the last 30 days.")}
                hint={t(
                  "Requests you or others decided, and your agents' requests, stay here for 30 days.",
                )}
              />
            )
          ) : (
            <PlanTable
              rows={list.rows}
              tab={tab}
              names={names}
              open={open}
              onOpen={(id) => put({ id })}
            />
          )}
          {list.kind === "ok" && list.total > list.rows.length && (
            <div className="appr-more">
              <p className="hint">
                {t("Showing {shown} of {total}.", { shown: list.rows.length, total: list.total })}
              </p>
              {list.more === "failed" && (
                <p className="hint" role="alert">
                  {t("The approvals could not be loaded.")}
                </p>
              )}
              <Button loading={list.more === "busy"} onClick={() => void more()}>
                {t("Show more")}
              </Button>
            </div>
          )}
        </div>
      </section>
      {open && (
        <PlanPanel
          key={open}
          id={open}
          names={names}
          onClose={() => put({})}
          onDecided={() => {
            decided.current = true;
            put({});
            void load();
          }}
          onStale={() => void load(true)}
        />
      )}
    </>
  );
}

/**
 * The list: a table on a tablet and a laptop, one block per row on a phone. A row has one control,
 * the button that opens its plan; it covers the whole row.
 */
function PlanTable({
  rows,
  tab,
  names,
  open,
  onOpen,
}: {
  rows: PlanRow[];
  tab: ApprovalsTab;
  names: Names;
  open: string;
  onOpen: (id: string) => void;
}) {
  const waiting = tab === "waiting";
  return (
    <Table
      caption={waiting ? t("Requests waiting for you") : t("Decided requests")}
      stack
      className="appr-table"
    >
      <thead>
        <tr>
          <th scope="col">{t("Request")}</th>
          <th scope="col">{t("From")}</th>
          {waiting ? (
            <>
              <th scope="col">{t("Asked")}</th>
              <th scope="col">{t("Expires")}</th>
            </>
          ) : (
            <>
              <th scope="col">{t("Outcome")}</th>
              <th scope="col">{t("Decided")}</th>
            </>
          )}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row.id} aria-current={open === row.id || undefined}>
            <td className="cell-main">
              <button
                type="button"
                className="appr-open"
                data-plan={row.id}
                aria-expanded={open === row.id}
                onClick={() => onOpen(row.id)}
              >
                <span className="appr-thing">{thingOf(row, names)}</span>
                <span className="appr-sub">
                  {waiting
                    ? waitingLine(row)
                    : `${statusWord(row.status)} · ${decidedBy(row)} · ${whenOf(row.decided_at)}`}
                </span>
              </button>
              {row.destructive && waiting && (
                <Badge tone="danger" dot={false} className="appr-chip">
                  {t("Cannot be undone")}
                </Badge>
              )}
            </td>
            <td className="appr-col">{whoOf(row)}</td>
            {waiting ? (
              <>
                <td className="appr-col">{whenOf(row.created_at)}</td>
                <td className="appr-col">{row.expires_at ? whenOf(row.expires_at) : "—"}</td>
              </>
            ) : (
              <>
                <td className="appr-col">{statusWord(row.status)}</td>
                <td className="appr-col">
                  {decidedBy(row)} · {whenOf(row.decided_at)}
                </td>
              </>
            )}
          </tr>
        ))}
      </tbody>
    </Table>
  );
}

/** Who decided a plan; nobody for one that expired. */
const decidedBy = (row: PlanRow): string =>
  row.decided_by || (row.status === "expired" ? t("Nobody") : t("Somebody"));
