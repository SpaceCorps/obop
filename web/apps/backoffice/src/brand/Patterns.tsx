import { useCallback, useMemo, useState } from "react";
import {
  Badge,
  Button,
  EmptyState,
  Icon,
  IconButton,
  Input,
  Kbd,
  Select,
  Switch,
  Textarea,
} from "@obop/ui";
import type { FieldMeta, ModelMeta, ModuleMeta, Row } from "../api.ts";
import { ControlPanel, type ViewDef } from "../ControlPanel.tsx";
import { FilterPanel } from "../FilterPanel.tsx";
import { DataProvider, useData } from "../data.tsx";
import { ConfirmDialog, type ConfirmRequest } from "../ui.tsx";
import { emptyState, type ViewState } from "../query.ts";
import { KanbanView } from "../views/KanbanView.tsx";
import { ListView } from "../views/ListView.tsx";
import type { ViewProps } from "../views/types.ts";
import { Block, Specimen } from "./kit.tsx";

type Notify = (t: string, tone?: "ok" | "error" | "info") => void;

const f = (name: string, kind: FieldMeta["kind"], extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind,
  description: "",
  class: "internal",
  personal_data: false,
  required: false,
  ...extra,
});

/** A made-up model, so the patterns run on sample data and never touch the instance. */
const MODEL: ModelMeta = {
  name: "sample_deal",
  description: "Sample deals for the brand book",
  can_create: true,
  can_update: true,
  actions: [],
  fields: [
    f("title", "text", { required: true }),
    f("company", "text"),
    f("amount_minor", "money"),
    f("stage", "select", { options: ["new", "qualified", "proposal", "won"] }),
    f("owner", "text"),
  ],
};

const SAMPLE_MODULES: ModuleMeta[] = [{ id: "sample", name: "Sample", models: [MODEL] }];
const SAMPLE_APPS: never[] = [];
const noLink = () => null;
const alwaysYes = async () => true;

const SEED: Row[] = [
  ["Office refit, 40 seats", "Northwind Traders", 1250000, "new", "Ada"],
  ["Annual support renewal", "Globex Corporation", 498050, "qualified", "Grace"],
  ["Warehouse automation pilot", "Initech", 3200000, "proposal", "Linus"],
  ["Regional distributor", "Hooli", 2400000, "qualified", "Margaret"],
  ["Fleet telematics rollout", "Umbrella Logistics", 1800000, "won", "Alan"],
  ["Training programme Q3", "Stark Supply", 640000, "new", "Katherine"],
  ["Retail pop-up fit-out", "Wayne Freight", 925000, "proposal", "Dennis"],
].map(([title, company, amount_minor, stage, owner], i) => ({
  id: `sample-${i}`,
  title,
  company,
  amount_minor,
  stage,
  owner,
}));

const VIEWS: ViewDef[] = [
  { id: "list", label: "List", icon: "list" },
  { id: "kanban", label: "Kanban", icon: "kanban" },
  { id: "pivot", label: "Pivot", icon: "pivot" },
  { id: "graph", label: "Graph", icon: "graph" },
];

function useSample(notify: Notify) {
  const [rows, setRows] = useState<Row[]>(SEED);
  const [selected, setSelected] = useState("");
  const update = useCallback(async (id: string, patch: Record<string, unknown>) => {
    setRows((r) => r.map((x) => (x.id === id ? { ...x, ...patch } : x)));
    return null;
  }, []);
  const props: ViewProps = {
    model: MODEL,
    title: "Deal",
    fields: MODEL.fields,
    rows,
    state: emptyState(),
    setState: () => {},
    selectedId: selected,
    onOpen: (id) => {
      setSelected(id);
      notify("In the product this opens the record panel");
    },
    canCreate: true,
    canUpdate: true,
    update,
    create: async (body) => {
      setRows((r) => [...r, { id: `sample-${r.length}`, ...body } as Row]);
      return null;
    },
    newWith: () => notify("In the product this opens the full form"),
    drill: () => {},
  };
  return props;
}

function ControlPanelDemo() {
  const d = useData();
  const [state, setState] = useState<ViewState>({
    facets: [{ op: "is", f: "stage", v: ["qualified"] }],
    group: [],
  });
  const [view, setView] = useState("list");
  return (
    <Specimen
      title="Control panel"
      note="Breadcrumb and title, the primary action, one search box with facets and the filter panel, and the view switcher. Everything here is the real component."
      wide
    >
      <div className="bb-frame panel open-frame">
        <ControlPanel
          appName="Sales"
          title="Deal"
          model="sample_deal"
          fields={MODEL.fields}
          canCreate
          onNew={() => {}}
          state={state}
          setState={setState}
          lookup={d.lookup}
          views={VIEWS}
          view={view}
          setView={setView}
          applyFavorite={() => {}}
          count={SEED.length}
        />
      </div>
    </Specimen>
  );
}

function FilterDemo() {
  const [state, setState] = useState<ViewState>(emptyState());
  return (
    <Specimen
      title="Filter, group and favourites"
      note="The dropdown under the search box: quick filters, grouping and saved searches side by side."
      wide
    >
      <div className="panel filter-demo">
        <FilterPanel
          model="sample_deal"
          fields={MODEL.fields}
          state={state}
          view="list"
          setState={setState}
          applyFavorite={() => {}}
        />
      </div>
    </Specimen>
  );
}

function KanbanDemo({ props }: { props: ViewProps }) {
  return (
    <Specimen
      title="Kanban column and card"
      note="Drag a card to another column, or focus it and press Shift and the arrow keys; every card also has a Move to select, so nothing needs a drag (WCAG 2.5.7)."
      wide
    >
      <div className="bb-frame panel kanban-frame">
        <div className="content view-kanban">
          <KanbanView {...props} state={{ facets: [], group: ["stage"] }} />
        </div>
      </div>
    </Specimen>
  );
}

function ListDemo({ props }: { props: ViewProps }) {
  return (
    <Specimen
      title="List rows"
      note="Click a cell to edit in place; the first column opens the record. Arrow keys move between cells, Space selects, F2 edits."
      wide
    >
      <div className="bb-frame panel list-frame">
        <div className="content view-list">
          <ListView {...props} />
        </div>
      </div>
    </Specimen>
  );
}

function RecordDemo() {
  const steps = ["new", "qualified", "proposal", "won"];
  const [stage, setStage] = useState("qualified");
  const current = steps.indexOf(stage);
  return (
    <Specimen
      title="Record side panel"
      note="Records open beside the list, so context survives. The status bar is the workflow; the form bar sticks to the bottom."
      wide
    >
      <div className="record-demo">
        <aside className="record panel static" aria-label="Sample deal record">
          <header>
            <div>
              <h2>Warehouse automation pilot</h2>
              <span className="hint">Deal</span>
            </div>
            <IconButton icon="close" label="Close sample record" tooltip={false} />
          </header>
          <ol className="statusbar" aria-label="Stage">
            {steps.map((s, i) => (
              <li key={s}>
                <button
                  type="button"
                  className={s === stage ? "on" : i < current ? "done" : ""}
                  aria-current={s === stage ? "step" : undefined}
                  onClick={() => setStage(s)}
                >
                  {s[0]!.toUpperCase() + s.slice(1)}
                </button>
              </li>
            ))}
          </ol>
          <div className="smart" role="group" aria-label="Related records">
            <span className="smartbtn">Sale orders (2)</span>
            <span className="smartbtn">Invoices (1)</span>
          </div>
          <div className="actionrow">
            <Button>Send quote</Button>
            <Button>Mark as won</Button>
          </div>
          <form className="fields" onSubmit={(e) => e.preventDefault()}>
            <Input label="Title" defaultValue="Warehouse automation pilot" required />
            <Input label="Company" defaultValue="Initech" />
            <Input
              label={
                <>
                  Amount <Icon name="shield" size={13} className="shield" label="Financial data" />
                </>
              }
              defaultValue="32,000.00"
              className="money"
              inputMode="decimal"
            />
            <Select label="Owner" defaultValue="linus">
              <option value="linus">Linus</option>
              <option value="ada">Ada</option>
            </Select>
            <Textarea label="Notes" placeholder="Anything the team should know" />
            <Switch label="Follow this deal" defaultChecked />
            <footer className="formbar">
              <Button type="submit" variant="primary">
                Save
              </Button>
              <Button>Discard</Button>
              <span className="grow" />
              <span className="hint kbd-hint">
                <Kbd>⌘S</Kbd> save <Kbd>Esc</Kbd> close
              </span>
            </footer>
          </form>
        </aside>
        <div className="record-notes">
          <h4>Anatomy</h4>
          <ul className="bb-list">
            <li>Sticky header with the record name and a close button.</li>
            <li>Status bar: finished steps tinted, the current step solid.</li>
            <li>Smart buttons jump to related records with a live count.</li>
            <li>Fields in one column, labels above, masked data marked with a shield.</li>
            <li>Sticky form bar: Save is primary and disabled until something changes.</li>
          </ul>
        </div>
      </div>
    </Specimen>
  );
}

function EmptyDemo() {
  return (
    <Specimen
      title="Empty states"
      note="Say what is missing, why it matters and the one thing to do next. Never a blank panel."
      wide
    >
      <div className="empty-grid">
        <div className="panel">
          <EmptyState
            icon="users"
            title="No leads yet"
            hint="Records you create show up here."
            action={{ label: "Create the first lead", onClick: () => {} }}
          />
        </div>
        <div className="panel">
          <EmptyState
            icon="search"
            title="No matches"
            hint="No record fits the current search."
            action={{ label: "Clear filters", onClick: () => {} }}
          />
        </div>
        <div className="panel">
          <EmptyState
            icon="warning"
            title="Could not load records"
            hint="The server did not answer. Check your connection, then try again."
            action={{ label: "Try again", onClick: () => {} }}
          />
        </div>
      </div>
    </Specimen>
  );
}

function DialogDemo({ notify }: { notify: Notify }) {
  const [req, setReq] = useState<ConfirmRequest | null>(null);
  const ask = (message: string, confirmLabel: string, cancelLabel: string, done: string) =>
    setReq({
      message,
      confirmLabel,
      cancelLabel,
      resolve: (ok) => {
        setReq(null);
        if (ok) notify(done);
      },
    });
  return (
    <Specimen
      title="Toasts and confirmation"
      note="Toasts confirm and fade. A confirmation names the consequence and puts the safe choice first and focused; the destructive button says what it does."
    >
      <div className="state-row">
        <Button onClick={() => notify("Invoice INV-0042 posted")}>Success toast</Button>
        <Button
          onClick={() =>
            notify("Could not save the lead. Check your connection and try again.", "error")
          }
        >
          Error toast
        </Button>
        <Button
          onClick={() =>
            notify("Approval needed: a human must approve posting INV-0042 first.", "info")
          }
        >
          Approval toast
        </Button>
        <Button
          variant="danger"
          onClick={() =>
            ask(
              "Delete 3 leads? This cannot be undone.",
              "Delete 3 leads",
              "Keep leads",
              "3 leads deleted (sample only)",
            )
          }
        >
          Delete…
        </Button>
        <Button
          onClick={() =>
            ask(
              "You have unsaved changes. Leave and discard them?",
              "Leave and discard",
              "Keep editing",
              "Changes discarded (sample only)",
            )
          }
        >
          Leave with changes…
        </Button>
      </div>
      {req && <ConfirmDialog req={req} />}
    </Specimen>
  );
}

function AgentDemo() {
  return (
    <Specimen
      title="Agent-ready states"
      note="Masked values and approvals are part of the product, so they look like part of the product."
    >
      <div className="agent-demo">
        <div className="maskedfield">
          <span className="masked">NAME_2</span>
          <span className="hint">Masked for this session</span>
        </div>
        <div className="maskedfield">
          <span className="masked">EMAIL_1</span>
          <Badge tone="warning">Personal data</Badge>
        </div>
        <div className="notice warning" role="status">
          <Icon name="shield-check" size={18} />
          <div className="notice-body">
            <strong>Waiting for a human to approve</strong>
            <p>post invoice INV-0042, proposed by the sales agent.</p>
          </div>
          <Button size="sm" variant="primary">
            Review
          </Button>
        </div>
      </div>
    </Specimen>
  );
}

export function Patterns({ notify }: { notify: Notify }) {
  const props = useSample(notify);
  const lead = useMemo(
    () =>
      "Patterns are the real components running on sample data, in the current theme. Change the accent, mode, density or corners and watch them follow.",
    [],
  );
  return (
    <Block id="patterns" title="Patterns" lead={lead}>
      {/* A private data layer: the patterns run on the sample model and never reach the instance. */}
      <DataProvider
        token=""
        modules={SAMPLE_MODULES}
        apps={SAMPLE_APPS}
        notify={notify}
        confirm={alwaysYes}
        linkFor={noLink}
      >
        <div className="spec-stack">
          <ControlPanelDemo />
          <FilterDemo />
          <ListDemo props={props} />
          <KanbanDemo props={props} />
          <RecordDemo />
          <EmptyDemo />
          <DialogDemo notify={notify} />
          <AgentDemo />
        </div>
      </DataProvider>
    </Block>
  );
}
