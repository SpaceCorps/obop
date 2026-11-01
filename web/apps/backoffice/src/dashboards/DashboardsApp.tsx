import { useMemo, useState } from "react";
import {
  Button,
  ConfirmDialog,
  Dialog,
  EmptyState,
  Icon,
  Input,
  Notice,
  Skeleton,
  Switch,
  Tabs,
  Textarea,
  compareText,
  inSentence,
  panelId,
  rich,
  t,
  useI18n,
} from "@obop/ui";
import type { ModelMeta, Reply, Row } from "../api.ts";
import type { App } from "../Chrome.tsx";
import { useData, useRows } from "../data.tsx";
import { fieldTitle, modelTitle } from "../labels.ts";
import { serverMessage } from "../serverMessage.ts";
import type { Go, Route } from "../route.ts";
import { BOARD } from "./route.ts";
import { NoDashboards, WidgetCard } from "./WidgetCard.tsx";
import { WidgetForm, type WidgetValues } from "./WidgetForm.tsx";
import type { Dashboard, Widget } from "./widget.ts";

const byPosition = <T extends { position?: number | null; title: string }>(a: T, b: T) =>
  (a.position ?? 1e9) - (b.position ?? 1e9) || compareText(a.title, b.title);

/** A sensible first dashboard from what is installed: counts per model, money totals, one chart. */
export function starterWidgets(apps: App[]): WidgetValues[] {
  const models: { app: App; model: ModelMeta }[] = apps
    .filter((a) => a.id !== "customize" && a.id !== "dashboards")
    .flatMap((app) => app.module.models.map((model) => ({ app, model })));
  const name = (x: { app: App; model: ModelMeta }) => modelTitle(x.model, x.app.module.id);
  const out: WidgetValues[] = [];
  const base = { aggregate: "count" as const, measure_field: null, group_by: null, filter: null };
  // One count per app's first model, at most four, so the top row stays readable.
  for (const x of models
    .filter((m, i, all) => all.findIndex((y) => y.app === m.app) === i)
    .slice(0, 4))
    out.push({ ...base, title: name(x), kind: "kpi", model_name: x.model.name });
  // Money totals of the first two models that hold an amount.
  for (const x of models
    .filter((m) => m.model.fields.some((f) => f.kind === "money"))
    .slice(0, 2)) {
    const f = x.model.fields.find((g) => g.kind === "money")!;
    out.push({
      ...base,
      title: t("{field} of {model}", { field: fieldTitle(f), model: inSentence(name(x)) }),
      kind: "kpi",
      aggregate: "sum",
      measure_field: f.name,
      model_name: x.model.name,
    });
  }
  const withSelect = models.filter((m) => m.model.fields.some((f) => f.kind === "select"));
  const stateOf = (m: ModelMeta) =>
    m.fields.find((f) => f.kind === "select" && ["state", "stage", "status"].includes(f.name)) ??
    m.fields.find((f) => f.kind === "select")!;
  const first = withSelect[0];
  if (first) {
    const g = stateOf(first.model);
    out.push({
      ...base,
      title: t("{model} by {field}", { model: name(first), field: inSentence(fieldTitle(g)) }),
      kind: "bar",
      model_name: first.model.name,
      group_by: g.name,
    });
    const money = first.model.fields.find((f) => f.kind === "money");
    out.push({
      ...base,
      title: money
        ? t("{measure} by {field}", {
            measure: fieldTitle(money),
            field: inSentence(fieldTitle(g)),
          })
        : t("Share by {field}", { field: inSentence(fieldTitle(g)) }),
      kind: "pie",
      model_name: first.model.name,
      group_by: g.name,
      ...(money ? { aggregate: "sum" as const, measure_field: money.name } : {}),
    });
  }
  const second = withSelect[1] ?? withSelect[0];
  if (second) {
    const g = stateOf(second.model);
    out.push({
      ...base,
      title: t("{model} by {field}", { model: name(second), field: inSentence(fieldTitle(g)) }),
      kind: "table",
      model_name: second.model.name,
      group_by: g.name,
    });
  }
  return out;
}

function DashboardForm(p: {
  initial?: Dashboard;
  onSave: (v: { title: string; description: string; shared: boolean }) => Promise<string | null>;
  onClose: () => void;
}) {
  const [title, setTitle] = useState(p.initial?.title ?? "");
  const [description, setDescription] = useState(p.initial?.description ?? "");
  const [shared, setShared] = useState(p.initial?.shared ?? true);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function save() {
    if (!title.trim()) return setErr(t("Give the dashboard a title"));
    setBusy(true);
    const e = await p.onSave({ title: title.trim(), description: description.trim(), shared });
    setBusy(false);
    if (e) setErr(e);
  }
  return (
    <Dialog
      title={p.initial ? t("Dashboard details") : t("New dashboard")}
      onClose={p.onClose}
      footer={
        <>
          <Button onClick={p.onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} onClick={() => void save()}>
            {p.initial ? t("Save") : t("Create dashboard")}
          </Button>
        </>
      }
    >
      <form
        className="wform"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Input
          label={t("Title")}
          value={title}
          required
          onChange={(e) => {
            setTitle(e.target.value);
            setErr(null);
          }}
        />
        <Textarea
          label={t("Description")}
          value={description}
          rows={2}
          onChange={(e) => setDescription(e.target.value)}
        />
        <Switch
          label={t("Shared with everyone who can open Dashboards")}
          checked={shared}
          onChange={(e) => setShared(e.target.checked)}
        />
        {err && (
          <Notice tone="danger" title={t("Could not save")}>
            {err}
          </Notice>
        )}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}

/** The Dashboards app: tiles drawn from `dashboards_widget` records, with an edit mode. */
export function DashboardsApp({
  app,
  route,
  go,
  owner,
}: {
  app: App;
  route: Route;
  go: Go;
  owner: string;
}) {
  const d = useData();
  // The order of equal positions is by title, collated in the language of the region.
  const { locale } = useI18n();
  const boards = useRows("dashboards_dashboard");
  const tiles = useRows("dashboards_widget");
  const dmodel = d.models.get("dashboards_dashboard")?.model;
  const wmodel = d.models.get("dashboards_widget")?.model;
  const canEdit = !!wmodel?.can_create && !!wmodel.can_update && !!dmodel?.can_create;
  const edit = route.view === "edit" && canEdit;

  const [form, setForm] = useState<{ widget?: Widget } | null>(null);
  const [boardForm, setBoardForm] = useState<{ board?: Dashboard } | null>(null);
  const [removing, setRemoving] = useState<Widget | null>(null);
  const [removingBoard, setRemovingBoard] = useState<Dashboard | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const visible = useMemo(
    () =>
      (boards.rows as unknown as Dashboard[])
        .filter((b) => canEdit || b.shared !== false || b.owner === owner)
        .sort(byPosition),
    [boards.rows, canEdit, owner, locale],
  );
  const current = visible.find((b) => b.id === route.id) ?? visible[0];
  const widgets = useMemo(
    () =>
      (tiles.rows as unknown as Widget[])
        .filter((w) => w.dashboard_id === current?.id)
        .sort(byPosition),
    [tiles.rows, current?.id, locale],
  );

  const open = (id: string, view = route.view) => go({ app: app.id, model: BOARD, id, view });
  const message = (r: Reply): string | null =>
    r.kind === "ok"
      ? null
      : r.kind === "approval"
        ? t(
            "Approval needed: plan {plan} was proposed. Nothing changes until a human approves it.",
            {
              plan: r.planId.slice(0, 8),
            },
          )
        : serverMessage(r.message);

  async function createStarter() {
    setBusy(true);
    setFailure(null);
    const made = await d.run("dashboards_dashboard.create", {
      title: t("Overview"),
      owner,
      shared: true,
      position: 10,
      description: t("Counts and totals from the apps you have installed."),
    });
    if (made.kind !== "ok") {
      setBusy(false);
      return setFailure(message(made));
    }
    const id = (made.value as Row).id;
    const list = starterWidgets(d.apps);
    for (const [i, w] of list.entries()) {
      const r = await d.run("dashboards_widget.create", {
        ...w,
        dashboard_id: id,
        position: (i + 1) * 10,
      });
      if (r.kind !== "ok") {
        setFailure(message(r));
        break;
      }
    }
    setBusy(false);
    open(id, "");
  }

  async function saveWidget(values: WidgetValues, existing?: Widget): Promise<string | null> {
    if (!current) return t("No dashboard to add to");
    const next = Math.max(0, ...widgets.map((w) => w.position ?? 0)) + 10;
    const r = existing
      ? await d.run("dashboards_widget.update", { id: existing.id, ...values })
      : await d.run("dashboards_widget.create", {
          ...values,
          dashboard_id: current.id,
          position: next,
        });
    const m = message(r);
    if (!m) {
      d.notify(existing ? t("Tile saved") : t("Tile added"));
      setForm(null);
    }
    return m;
  }

  async function move(w: Widget, delta: -1 | 1) {
    const i = widgets.findIndex((x) => x.id === w.id);
    const j = i + delta;
    if (j < 0 || j >= widgets.length) return;
    const order = [...widgets];
    [order[i], order[j]] = [order[j]!, order[i]!];
    // Renumber everything that changed, so equal or missing positions cannot tie.
    for (const [k, x] of order.entries()) {
      const position = (k + 1) * 10;
      if (x.position !== position) {
        const r = await d.run("dashboards_widget.update", { id: x.id, position });
        if (r.kind !== "ok") return d.notify(message(r) ?? t("Could not move the tile"), "error");
      }
    }
  }

  async function removeWidget() {
    if (!removing) return;
    setBusy(true);
    const r = await d.run("dashboards_widget.delete", { id: removing.id });
    setBusy(false);
    const m = message(r);
    if (m) setFailure(m);
    else {
      d.notify(t("Removed “{title}”", { title: removing.title }));
      setRemoving(null);
    }
  }

  async function saveBoard(
    v: { title: string; description: string; shared: boolean },
    b?: Dashboard,
  ) {
    const body = { title: v.title, description: v.description || null, shared: v.shared };
    const r = b
      ? await d.run("dashboards_dashboard.update", { id: b.id, ...body })
      : await d.run("dashboards_dashboard.create", {
          ...body,
          owner,
          position: Math.max(0, ...visible.map((x) => x.position ?? 0)) + 10,
        });
    const m = message(r);
    if (!m) {
      setBoardForm(null);
      d.notify(b ? t("Dashboard saved") : t("Dashboard created"));
      if (!b && r.kind === "ok") open((r.value as Row).id, "edit");
    }
    return m;
  }

  async function removeBoard() {
    if (!removingBoard) return;
    setBusy(true);
    setFailure(null);
    // The server refuses to delete a dashboard that tiles still point at, so the tiles go first.
    for (const w of (tiles.rows as unknown as Widget[]).filter(
      (x) => x.dashboard_id === removingBoard.id,
    )) {
      const r = await d.run("dashboards_widget.delete", { id: w.id });
      if (r.kind !== "ok") {
        setBusy(false);
        return setFailure(message(r));
      }
    }
    const r = await d.run("dashboards_dashboard.delete", { id: removingBoard.id });
    setBusy(false);
    const m = message(r);
    if (m) return setFailure(m);
    d.notify(t("Deleted “{title}”", { title: removingBoard.title }));
    setRemovingBoard(null);
    go({ app: app.id, model: BOARD });
  }

  const loading = !boards.loaded || !tiles.loaded;

  return (
    <section className="main panel dash" aria-label={t("Dashboards")}>
      <div className="controlpanel">
        <div className="cp-top">
          <div className="crumbs">
            <span>{app.name}</span>
            <Icon name="chevron-right" size={12} />
            <strong>{current?.title ?? t("Dashboards")}</strong>
          </div>
          <div className="actionrow">
            {canEdit && current && !edit && (
              <Button
                icon="edit"
                onClick={() => go({ app: app.id, model: BOARD, id: current.id, view: "edit" })}
              >
                {t("Edit dashboard")}
              </Button>
            )}
            {edit && current && (
              <>
                <Button icon="plus" variant="primary" onClick={() => setForm({})}>
                  {t("Add a tile")}
                </Button>
                <Button onClick={() => setBoardForm({})}>{t("New dashboard")}</Button>
                <Button onClick={() => setBoardForm({ board: current })}>{t("Details")}</Button>
                <Button
                  className="danger-text"
                  variant="ghost"
                  onClick={() => setRemovingBoard(current)}
                >
                  {t("Delete dashboard")}
                </Button>
                <Button onClick={() => open(current.id, "")}>{t("Done")}</Button>
              </>
            )}
          </div>
        </div>
        {visible.length > 1 && (
          <Tabs
            label={t("Dashboards")}
            idPrefix="db"
            value={current?.id ?? ""}
            onChange={(id) => open(id)}
            tabs={visible.map((b) => ({ id: b.id, label: b.title }))}
          />
        )}
      </div>
      <div
        className="content pad"
        role={visible.length > 1 ? "tabpanel" : undefined}
        id={visible.length > 1 ? panelId("db") : undefined}
        aria-labelledby={visible.length > 1 && current ? `db-tab-${current.id}` : undefined}
      >
        {failure && (
          <Notice tone="danger" title={t("Something did not work")}>
            {failure}
          </Notice>
        )}
        {boards.error || tiles.error ? (
          <EmptyState
            title={t("Could not load dashboards")}
            hint={serverMessage(boards.error ?? tiles.error)}
          />
        ) : loading ? (
          <Skeleton rows={6} cols={3} />
        ) : !current ? (
          <NoDashboards busy={busy} onCreate={canEdit ? () => void createStarter() : undefined} />
        ) : (
          <>
            {current.description && <p className="hint dash-desc">{current.description}</p>}
            {widgets.length === 0 ? (
              <EmptyState
                icon="graph"
                title={t("This dashboard has no tiles")}
                hint={
                  canEdit
                    ? t("Add a number, a chart or a table.")
                    : t("Nothing has been added yet.")
                }
                action={
                  canEdit ? { label: t("Add a tile"), onClick: () => setForm({}) } : undefined
                }
              />
            ) : (
              <div className="wgrid">
                {widgets.map((w, i) => (
                  <WidgetCard
                    key={w.id}
                    widget={w}
                    edit={edit}
                    first={i === 0}
                    last={i === widgets.length - 1}
                    onEdit={() => setForm({ widget: w })}
                    onRemove={() => {
                      setFailure(null);
                      setRemoving(w);
                    }}
                    onMove={(delta) => void move(w, delta)}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </div>
      {form && (
        <WidgetForm
          initial={form.widget}
          onSave={(v) => saveWidget(v, form.widget)}
          onClose={() => setForm(null)}
        />
      )}
      {boardForm && (
        <DashboardForm
          initial={boardForm.board}
          onSave={(v) => saveBoard(v, boardForm.board)}
          onClose={() => setBoardForm(null)}
        />
      )}
      {removing && (
        <ConfirmDialog
          title={t("Remove tile?")}
          confirmLabel={t("Remove tile")}
          busy={busy}
          error={failure}
          onConfirm={() => void removeWidget()}
          onClose={() => setRemoving(null)}
        >
          <p>
            {rich(
              "Remove <b>{title}</b> from this dashboard? The records it shows are not touched.",
              { title: removing.title },
              { b: (c) => <strong>{c}</strong> },
            )}
          </p>
        </ConfirmDialog>
      )}
      {removingBoard && (
        <ConfirmDialog
          title={t("Delete dashboard?")}
          confirmLabel={t("Delete dashboard")}
          busy={busy}
          error={failure}
          onConfirm={() => void removeBoard()}
          onClose={() => setRemovingBoard(null)}
        >
          <p>
            {rich(
              "{count, plural, one {Delete <b>{title}</b> and its # tile? This cannot be undone.} other {Delete <b>{title}</b> and its # tiles? This cannot be undone.}}",
              {
                title: removingBoard.title,
                count: (tiles.rows as unknown as Widget[]).filter(
                  (x) => x.dashboard_id === removingBoard.id,
                ).length,
              },
              { b: (c) => <strong>{c}</strong> },
            )}
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}
