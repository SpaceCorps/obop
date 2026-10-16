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
  panelId,
} from "@obop/ui";
import type { ModelMeta, Reply, Row } from "../api.ts";
import type { App } from "../Chrome.tsx";
import { useData, useRows } from "../data.tsx";
import { label, modelLabel } from "../format.ts";
import type { Go, Route } from "../route.ts";
import { BOARD } from "./route.ts";
import { NoDashboards, WidgetCard } from "./WidgetCard.tsx";
import { WidgetForm, type WidgetValues } from "./WidgetForm.tsx";
import type { Dashboard, Widget } from "./widget.ts";

const byPosition = <T extends { position?: number | null; title: string }>(a: T, b: T) =>
  (a.position ?? 1e9) - (b.position ?? 1e9) || a.title.localeCompare(b.title);

/** A sensible first dashboard from what is installed: counts per model, money totals, one chart. */
export function starterWidgets(apps: App[]): WidgetValues[] {
  const models: { app: App; model: ModelMeta }[] = apps
    .filter((a) => a.id !== "customize" && a.id !== "dashboards")
    .flatMap((app) => app.module.models.map((model) => ({ app, model })));
  const name = (x: { app: App; model: ModelMeta }) => modelLabel(x.model.name, x.app.module.id);
  const out: WidgetValues[] = [];
  const base = { aggregate: "count" as const, measure_field: null, group_by: null, filter: null };
  // One count per app's first model, at most four, so the top row stays readable.
  for (const x of models
    .filter((m, i, all) => all.findIndex((y) => y.app === m.app) === i)
    .slice(0, 4))
    out.push({ ...base, title: `${name(x)}`, kind: "kpi", model_name: x.model.name });
  // Money totals of the first two models that hold an amount.
  for (const x of models
    .filter((m) => m.model.fields.some((f) => f.kind === "money"))
    .slice(0, 2)) {
    const f = x.model.fields.find((g) => g.kind === "money")!;
    out.push({
      ...base,
      title: `${label(f.name)} of ${name(x).toLowerCase()}`,
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
      title: `${name(first)} by ${label(g.name).toLowerCase()}`,
      kind: "bar",
      model_name: first.model.name,
      group_by: g.name,
    });
    const money = first.model.fields.find((f) => f.kind === "money");
    out.push({
      ...base,
      title: money
        ? `${label(money.name)} by ${label(g.name).toLowerCase()}`
        : `Share by ${label(g.name).toLowerCase()}`,
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
      title: `${name(second)} by ${label(g.name).toLowerCase()}`,
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
    if (!title.trim()) return setErr("Give the dashboard a title");
    setBusy(true);
    const e = await p.onSave({ title: title.trim(), description: description.trim(), shared });
    setBusy(false);
    if (e) setErr(e);
  }
  return (
    <Dialog
      title={p.initial ? "Dashboard details" : "New dashboard"}
      onClose={p.onClose}
      footer={
        <>
          <Button onClick={p.onClose}>Cancel</Button>
          <Button variant="primary" loading={busy} onClick={() => void save()}>
            {p.initial ? "Save" : "Create dashboard"}
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
          label="Title"
          value={title}
          required
          onChange={(e) => {
            setTitle(e.target.value);
            setErr(null);
          }}
        />
        <Textarea
          label="Description"
          value={description}
          rows={2}
          onChange={(e) => setDescription(e.target.value)}
        />
        <Switch
          label="Shared with everyone who can open Dashboards"
          checked={shared}
          onChange={(e) => setShared(e.target.checked)}
        />
        {err && (
          <Notice tone="danger" title="Could not save">
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
    [boards.rows, canEdit, owner],
  );
  const current = visible.find((b) => b.id === route.id) ?? visible[0];
  const widgets = useMemo(
    () =>
      (tiles.rows as unknown as Widget[])
        .filter((w) => w.dashboard_id === current?.id)
        .sort(byPosition),
    [tiles.rows, current?.id],
  );

  const open = (id: string, view = route.view) => go({ app: app.id, model: BOARD, id, view });
  const message = (r: Reply): string | null =>
    r.kind === "ok"
      ? null
      : r.kind === "approval"
        ? `Approval needed: plan ${r.planId.slice(0, 8)} was proposed. Nothing changes until a human approves it.`
        : r.message;

  async function createStarter() {
    setBusy(true);
    setFailure(null);
    const made = await d.run("dashboards_dashboard.create", {
      title: "Overview",
      owner,
      shared: true,
      position: 10,
      description: "Counts and totals from the apps you have installed.",
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
    if (!current) return "No dashboard to add to";
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
      d.notify(existing ? "Tile saved" : "Tile added");
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
        if (r.kind !== "ok") return d.notify(message(r) ?? "Could not move the tile", "error");
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
      d.notify(`Removed “${removing.title}”`);
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
      d.notify(b ? "Dashboard saved" : "Dashboard created");
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
    d.notify(`Deleted “${removingBoard.title}”`);
    setRemovingBoard(null);
    go({ app: app.id, model: BOARD });
  }

  const loading = !boards.loaded || !tiles.loaded;

  return (
    <section className="main panel dash" aria-label="Dashboards">
      <div className="controlpanel">
        <div className="cp-top">
          <div className="crumbs">
            <span>{app.name}</span>
            <Icon name="chevron-right" size={12} />
            <strong>{current?.title ?? "Dashboards"}</strong>
          </div>
          <div className="actionrow">
            {canEdit && current && !edit && (
              <Button
                icon="edit"
                onClick={() => go({ app: app.id, model: BOARD, id: current.id, view: "edit" })}
              >
                Edit dashboard
              </Button>
            )}
            {edit && current && (
              <>
                <Button icon="plus" variant="primary" onClick={() => setForm({})}>
                  Add a tile
                </Button>
                <Button onClick={() => setBoardForm({})}>New dashboard</Button>
                <Button onClick={() => setBoardForm({ board: current })}>Details</Button>
                <Button
                  className="danger-text"
                  variant="ghost"
                  onClick={() => setRemovingBoard(current)}
                >
                  Delete dashboard
                </Button>
                <Button onClick={() => open(current.id, "")}>Done</Button>
              </>
            )}
          </div>
        </div>
        {visible.length > 1 && (
          <Tabs
            label="Dashboards"
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
          <Notice tone="danger" title="Something did not work">
            {failure}
          </Notice>
        )}
        {boards.error || tiles.error ? (
          <EmptyState title="Could not load dashboards" hint={boards.error ?? tiles.error} />
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
                title="This dashboard has no tiles"
                hint={canEdit ? "Add a number, a chart or a table." : "Nothing has been added yet."}
                action={canEdit ? { label: "Add a tile", onClick: () => setForm({}) } : undefined}
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
          title="Remove tile?"
          confirmLabel="Remove tile"
          busy={busy}
          error={failure}
          onConfirm={() => void removeWidget()}
          onClose={() => setRemoving(null)}
        >
          <p>
            Remove <strong>{removing.title}</strong> from this dashboard? The records it shows are
            not touched.
          </p>
        </ConfirmDialog>
      )}
      {removingBoard && (
        <ConfirmDialog
          title="Delete dashboard?"
          confirmLabel="Delete dashboard"
          busy={busy}
          error={failure}
          onConfirm={() => void removeBoard()}
          onClose={() => setRemovingBoard(null)}
        >
          <p>
            Delete <strong>{removingBoard.title}</strong> and its{" "}
            {
              (tiles.rows as unknown as Widget[]).filter((x) => x.dashboard_id === removingBoard.id)
                .length
            }{" "}
            tiles? This cannot be undone.
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}
