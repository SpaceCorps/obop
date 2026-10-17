import { useState } from "react";
import {
  Avatar,
  Badge,
  Button,
  Card,
  Checkbox,
  Chip,
  CodeBlock,
  ConfirmDialog,
  Dialog,
  EmptyState,
  Icon,
  IconButton,
  Input,
  Kbd,
  Menu,
  Notice,
  Pager,
  Panel,
  Popover,
  QrCode,
  Segmented,
  Select,
  Skeleton,
  Spinner,
  Stat,
  Switch,
  Table,
  Tabs,
  Textarea,
  Toast,
  Tooltip,
  TwoFactorStep,
} from "@obop/ui";
import { Block, Specimen, State } from "./kit.tsx";

type Notify = (t: string, tone?: "ok" | "error" | "info") => void;

function Buttons() {
  const variants = ["primary", "secondary", "ghost", "danger"] as const;
  return (
    <Specimen
      title="Button"
      note="One primary action per view. Labels are verbs: Save, Create invoice, Delete 3 leads."
      wide
      props={[
        [
          "variant",
          '"primary" | "secondary" | "ghost" | "danger"',
          '"secondary"',
          "Visual weight. Danger is for destructive actions only.",
        ],
        [
          "size",
          '"sm" | "md" | "lg"',
          '"md"',
          "Control height: 30, 36 and 44px (24, 30 and 36px in compact density).",
        ],
        [
          "loading",
          "boolean",
          "false",
          "Shows a spinner, keeps the label, blocks clicks, sets aria-busy.",
        ],
        ["icon / iconEnd", "IconName", "none", "Leading or trailing icon."],
        [
          "…rest",
          "ButtonHTMLAttributes",
          "",
          "Everything a button accepts. Defaults to type=button.",
        ],
      ]}
    >
      <div className="state-grid">
        {variants.map((v) => (
          <div key={v} className="state-col">
            <span className="overline">{v}</span>
            <State label="Default">
              <Button variant={v}>Save</Button>
            </State>
            <State label="Hover">
              <Button variant={v} data-state="hover">
                Save
              </Button>
            </State>
            <State label="Focus">
              <Button variant={v} data-state="focus">
                Save
              </Button>
            </State>
            <State label="Disabled">
              <Button variant={v} disabled>
                Save
              </Button>
            </State>
            <State label="Loading">
              <Button variant={v} loading>
                Saving
              </Button>
            </State>
          </div>
        ))}
        <div className="state-col">
          <span className="overline">Sizes and icons</span>
          <State label="Small">
            <Button size="sm" variant="primary">
              Small
            </Button>
          </State>
          <State label="Medium">
            <Button variant="primary" icon="plus">
              New
            </Button>
          </State>
          <State label="Large">
            <Button size="lg" variant="primary" iconEnd="arrow-right">
              Continue
            </Button>
          </State>
        </div>
      </div>
    </Specimen>
  );
}

function IconButtons() {
  return (
    <Specimen
      title="IconButton"
      note="Needs a label. The label is the accessible name and the tooltip."
      props={[
        ["icon", "IconName", "required", "The glyph."],
        ["label", "string", "required", "Accessible name and tooltip text."],
        ["size", '"sm" | "md"', '"md"', "30 or 36px square; 24 or 30px in compact density."],
        ["tooltip", "boolean", "true", "Turn off when the label is visible next to it."],
      ]}
    >
      <div className="state-row">
        <State label="Default">
          <IconButton icon="trash" label="Delete record" />
        </State>
        <State label="Hover">
          <IconButton icon="trash" label="Delete record" data-state="hover" tooltip={false} />
        </State>
        <State label="Focus">
          <IconButton icon="trash" label="Delete record" data-state="focus" tooltip={false} />
        </State>
        <State label="Pressed">
          <IconButton icon="star" label="Favourite" aria-pressed="true" tooltip={false} />
        </State>
        <State label="Disabled">
          <IconButton icon="edit" label="Edit record" disabled />
        </State>
      </div>
    </Specimen>
  );
}

function Fields() {
  return (
    <Specimen
      title="Input, Select, Textarea"
      note="A label is always visible. Hints explain; errors say what to do."
      wide
      props={[
        ["label", "ReactNode", "none", "Visible label, wired to the control with htmlFor."],
        ["hint", "ReactNode", "none", "Help below the control; linked with aria-describedby."],
        [
          "error",
          "ReactNode",
          "none",
          "Error below the control; sets aria-invalid and the red outline.",
        ],
        ["required", "boolean", "false", "Adds the asterisk and the required attribute."],
        ["…rest", "InputHTMLAttributes", "", "Everything the native element accepts."],
      ]}
    >
      <div className="field-grid">
        <Input label="Name" placeholder="Acme Trading" hint="As it appears on invoices." />
        <Input label="Name (hover)" placeholder="Acme Trading" data-state="hover" />
        <Input label="Name (focus)" defaultValue="Acme Trading" data-state="focus" />
        <Input
          label="Email"
          defaultValue="ada@"
          error="Enter an email like name@company.com."
          required
        />
        <Input label="Tax id" defaultValue="DE123456789" disabled />
        <Input label="Reference" defaultValue="INV-0042" readOnly />
        <Select label="Stage" defaultValue="qualified">
          <option value="new">New</option>
          <option value="qualified">Qualified</option>
          <option value="won">Won</option>
        </Select>
        <Select label="Stage (error)" error="Pick a stage before you save." defaultValue="">
          <option value="" disabled>
            Choose…
          </option>
          <option value="new">New</option>
        </Select>
        <Textarea label="Notes" placeholder="Anything the team should know" hint="Plain text." />
      </div>
    </Specimen>
  );
}

function Toggles() {
  const [on, setOn] = useState(true);
  return (
    <Specimen
      title="Checkbox and Switch"
      note="Checkbox for choosing several; Switch for something that takes effect at once."
      props={[
        ["label", "ReactNode", "required", "The visible label; the whole row is the click target."],
        ["indeterminate", "boolean", "false", "Checkbox only: partly selected (select-all)."],
        ["…rest", "InputHTMLAttributes", "", "checked, defaultChecked, onChange, disabled…"],
      ]}
    >
      <div className="toggle-grid">
        <div className="stack">
          <Checkbox label="Unchecked" />
          <Checkbox label="Checked" defaultChecked />
          <Checkbox label="Indeterminate" indeterminate />
          <Checkbox label="Hover" data-state="hover" />
          <Checkbox label="Focus" data-state="focus" />
          <Checkbox label="Disabled" disabled />
          <Checkbox label="Disabled, checked" disabled defaultChecked />
        </div>
        <div className="stack">
          <Switch
            label={on ? "On" : "Off"}
            checked={on}
            onChange={(e) => setOn(e.target.checked)}
          />
          <Switch label="Focus" data-state="focus" defaultChecked />
          <Switch label="Disabled" disabled />
          <Switch label="Disabled, on" disabled defaultChecked />
        </div>
      </div>
    </Specimen>
  );
}

function Labels() {
  return (
    <Specimen
      title="Badge, Chip, Kbd, Avatar, Spinner"
      note="Badges state status and always carry a word. Chips are removable filters or tags."
      props={[
        [
          "Badge tone",
          '"neutral" | "accent" | "success" | "warning" | "danger" | "info"',
          '"neutral"',
          "Status colour; text is the tone's ink colour on a 14% tint.",
        ],
        [
          "Chip onRemove",
          "() => void",
          "none",
          "Adds a remove button (needs removeLabel for screen readers).",
        ],
        ["Avatar name", "string", "required", "Initials are derived; decorative (aria-hidden)."],
        [
          "Spinner label",
          "string",
          "none",
          "With a label it is a status region; without, decorative.",
        ],
      ]}
    >
      <div className="label-grid">
        <div className="chip-row">
          <Badge>Draft</Badge>
          <Badge tone="accent">Open</Badge>
          <Badge tone="success">Paid</Badge>
          <Badge tone="warning">In progress</Badge>
          <Badge tone="danger">Lost</Badge>
          <Badge tone="info">Sent</Badge>
        </div>
        <div className="chip-row">
          <Chip>Plain tag</Chip>
          <Chip tone="accent">Stage: Won</Chip>
          <Chip tone="accent" onRemove={() => {}} removeLabel="Remove filter Stage: Won">
            Stage: Won
          </Chip>
        </div>
        <div className="chip-row">
          <Kbd>⌘K</Kbd>
          <Kbd>Esc</Kbd>
          <Kbd>Shift</Kbd>
          <Avatar name="Ada Lovelace" size={28} />
          <Avatar name="grace@example.com" size={36} />
          <Spinner size={18} label="Loading" />
          <Spinner size={28} />
        </div>
      </div>
    </Specimen>
  );
}

function Surfaces() {
  return (
    <Specimen
      title="Card and Panel"
      note="Panel is the one framed surface (list, record, dialog). Card is for a thing you can pick."
      props={[
        [
          "Panel as",
          '"div" | "section" | "aside" | …',
          '"div"',
          "The element to render; use a landmark when it is one.",
        ],
        [
          "Card interactive",
          "boolean",
          "false",
          "Adds the hover lift; wrap the whole card in a link or button.",
        ],
      ]}
    >
      <div className="card-grid">
        <Panel className="demo-pad">
          <h4>Panel</h4>
          <p className="hint">Border, radius-lg, shadow-1 on the surface colour.</p>
        </Panel>
        <Card interactive>
          <h4>Interactive card</h4>
          <p className="hint">Lifts on hover. Border strengthens, shadow grows to level 2.</p>
        </Card>
      </div>
    </Specimen>
  );
}

function Navigation() {
  const [tab, setTab] = useState("details");
  const [view, setView] = useState("list");
  const [mode, setMode] = useState("light");
  return (
    <Specimen
      title="Tabs and Segmented"
      note="Tabs switch panels of one page. Segmented is a compact choice or a view switcher. Arrow keys move; Home and End jump."
      props={[
        [
          "Tabs tabs / value / onChange",
          "TabItem[] / id / (id) => void",
          "required",
          "Underlined tab list with roving tabindex.",
        ],
        [
          "Tabs idPrefix",
          "string",
          "required",
          "Links each tab to its panel (aria-controls, aria-labelledby).",
        ],
        [
          "Segmented mode",
          '"radio" | "toggle"',
          '"radio"',
          "radio = radiogroup, toggle = buttons with aria-pressed.",
        ],
        [
          "Segmented iconOnlyNarrow",
          "boolean",
          "false",
          "Hide text on narrow screens, keep the aria-label.",
        ],
      ]}
    >
      <div className="stack">
        <Tabs
          label="Example tabs"
          idPrefix="bb-tabs"
          value={tab}
          onChange={setTab}
          tabs={[
            { id: "details", label: "Details", icon: "list" },
            { id: "activity", label: "Activity", count: 12 },
            { id: "files", label: "Files" },
          ]}
        />
        <div className="state-row">
          <Segmented
            label="Choose a mode"
            value={mode}
            onChange={setMode}
            options={[
              { value: "light", label: "Light", icon: "sun" },
              { value: "dark", label: "Dark", icon: "moon" },
            ]}
          />
          <Segmented
            mode="toggle"
            label="View"
            value={view}
            onChange={setView}
            options={[
              { value: "list", label: "List", icon: "list" },
              { value: "kanban", label: "Kanban", icon: "kanban" },
              { value: "graph", label: "Graph", icon: "graph" },
            ]}
          />
        </div>
      </div>
    </Specimen>
  );
}

function Overlays({ notify }: { notify: Notify }) {
  const [dialog, setDialog] = useState(false);
  const [sheet, setSheet] = useState(false);
  const [confirm, setConfirm] = useState(false);
  return (
    <Specimen
      title="Menu, Popover, Tooltip"
      note="Menu is for actions, Popover for any floating content. Both close on Escape and return focus to the trigger."
      props={[
        [
          "ConfirmDialog",
          "title, confirmLabel, onConfirm, onClose, error?, busy?",
          "none",
          "The in-app replacement for window.confirm. It states the consequence, keeps focus on Cancel, and stays open to show a failure in its alert area.",
        ],
        [
          "Menu items",
          "MenuItem[]",
          "required",
          "Actions, separators and headings; arrow keys move focus.",
        ],
        [
          "MenuItem description",
          "string",
          "none",
          "A second line under the label and its tooltip. Say why an item is disabled, in a sentence.",
        ],
        [
          "Menu align",
          '"start" | "end"',
          '"start"',
          "Which edge of the trigger the panel lines up with.",
        ],
        [
          "Popover children",
          "(close) => ReactNode",
          "required",
          "Render the panel; call close() to dismiss.",
        ],
        [
          "Tooltip label",
          "string",
          "required",
          "Shows after 350ms of hover or on keyboard focus; Escape hides it. A sentence wraps.",
        ],
        [
          "Tooltip align",
          '"center" | "start" | "end"',
          '"center"',
          "Line the tip up with an edge of its target, for controls near the edge of a panel. A control that is unavailable stays focusable (aria-disabled) so its tooltip can say why.",
        ],
      ]}
    >
      <div className="state-row overlays">
        <Menu
          trigger={
            <>
              Actions <Icon name="chevron-down" size={14} />
            </>
          }
          items={[
            { heading: "Invoice INV-0042" },
            { label: "Duplicate", icon: "copy", onSelect: () => notify("Duplicated") },
            { label: "Download", icon: "download", onSelect: () => notify("Downloaded") },
            {
              label: "Post",
              icon: "check",
              disabled: true,
              description: "No action moves an invoice from Paid to Posted",
              onSelect: () => {},
            },
            "separator",
            {
              label: "Delete",
              icon: "trash",
              danger: true,
              onSelect: () => notify("Deleted", "error"),
            },
          ]}
        />
        <Popover label="Popover">
          {(close) => (
            <div className="stack">
              <p>Any content can float here.</p>
              <Button size="sm" onClick={close}>
                Got it
              </Button>
            </div>
          )}
        </Popover>
        <Tooltip label="Delete record">
          <Button variant="ghost" aria-label="Delete record">
            <Icon name="trash" size={16} />
          </Button>
        </Tooltip>
        <Button onClick={() => setDialog(true)}>Open dialog</Button>
        <Button onClick={() => setSheet(true)}>Open sheet</Button>
        <Button onClick={() => setConfirm(true)}>Open confirmation</Button>
      </div>
      {confirm && (
        <ConfirmDialog
          title="Delete lead?"
          confirmLabel="Delete"
          onClose={() => setConfirm(false)}
          onConfirm={() => (setConfirm(false), notify("Lead deleted"))}
        >
          <p>Delete lead “Annual support renewal”? This cannot be undone.</p>
        </ConfirmDialog>
      )}
      <Dialog
        open={dialog}
        onClose={() => setDialog(false)}
        title="Archive this project?"
        footer={
          <>
            <Button onClick={() => setDialog(false)}>Cancel</Button>
            <Button
              variant="primary"
              onClick={() => (setDialog(false), notify("Project archived"))}
            >
              Archive
            </Button>
          </>
        }
      >
        <p>
          Archived projects leave the board but keep their tasks and history. You can restore it
          later.
        </p>
      </Dialog>
      <Dialog open={sheet} variant="sheet" onClose={() => setSheet(false)} title="Filters">
        <div className="stack">
          <Select label="Stage" defaultValue="">
            <option value="">Any</option>
            <option>Qualified</option>
          </Select>
          <Checkbox label="Only my records" />
          <Button variant="primary" onClick={() => setSheet(false)}>
            Apply
          </Button>
        </div>
      </Dialog>
    </Specimen>
  );
}

function Feedback({ notify }: { notify: Notify }) {
  return (
    <Specimen
      title="Toast and Notice"
      note="Toasts confirm what just happened and leave by themselves. Notices stay until the problem is gone."
      props={[
        [
          "Toast tone",
          '"ok" | "error" | "info"',
          '"ok"',
          "Icon and ink colour. Errors stay 7s, others 3.5s.",
        ],
        [
          "Notice tone",
          '"info" | "success" | "warning" | "danger"',
          '"info"',
          "Danger is announced as an alert, the rest politely.",
        ],
      ]}
    >
      <div className="stack">
        <div className="toast-stack">
          <Toast tone="ok" onDismiss={() => {}}>
            Invoice INV-0042 posted
          </Toast>
          <Toast tone="error" onDismiss={() => {}}>
            Could not save the lead. Check your connection and try again.
          </Toast>
          <Toast tone="info" onDismiss={() => {}}>
            Approval needed: a human must approve this change first.
          </Toast>
        </div>
        <div className="state-row">
          <Button size="sm" onClick={() => notify("Saved for you")}>
            Show a toast
          </Button>
          <Button size="sm" onClick={() => notify("Could not save. Try again.", "error")}>
            Show an error
          </Button>
        </div>
        <Notice tone="info" title="Heads up">
          Quotes expire after 30 days unless you extend them.
        </Notice>
        <Notice tone="success" title="Backup finished">
          Your data was exported at 09:41.
        </Notice>
        <Notice tone="warning" title="Two invoices are overdue">
          Send a reminder before Friday.
        </Notice>
        <Notice tone="danger" title="Payment failed">
          The card was declined. Use another card to keep your plan.
        </Notice>
      </div>
    </Specimen>
  );
}

function DataDisplay() {
  return (
    <Specimen
      title="Table, Skeleton, EmptyState"
      note="Numbers are right-aligned and tabular. Loading uses a skeleton the shape of the content, never a lone spinner."
      wide
      props={[
        ["Table caption", "string", "none", "Screen-reader caption (visually hidden)."],
        ["Table dense", "boolean", "false", "Tighter rows for reference tables."],
        ["Skeleton rows / cols", "number", "6 / 5", "Placeholder shape."],
        [
          "EmptyState action",
          "{ label, onClick }",
          "none",
          "Primary next step. Say what is missing and how to fix it.",
        ],
      ]}
    >
      <div className="data-grid">
        <Table caption="Sample invoices">
          <thead>
            <tr>
              <th scope="col">Number</th>
              <th scope="col">Customer</th>
              <th scope="col">State</th>
              <th scope="col" className="num">
                Total
              </th>
            </tr>
          </thead>
          <tbody>
            {[
              ["INV-0042", "Northwind Traders", "Paid", "success", "€1,250.00"],
              ["INV-0043", "Globex Corporation", "Sent", "info", "€4,980.50"],
              ["INV-0044", "Initech", "Overdue", "danger", "€320.00"],
              ["INV-0045", "Hooli", "Draft", "neutral", "€12,000.00"],
            ].map(([n, c, s, t, a]) => (
              <tr key={n}>
                <td>{n}</td>
                <td>{c}</td>
                <td>
                  <Badge tone={t as "success"}>{s}</Badge>
                </td>
                <td className="num tabular">{a}</td>
              </tr>
            ))}
          </tbody>
        </Table>
        <Panel className="demo-pad">
          <span className="overline">Loading</span>
          <Skeleton rows={3} cols={3} />
        </Panel>
        <Panel className="demo-pad">
          <EmptyState
            icon="users"
            title="No leads yet"
            hint="Create the first one to start your pipeline."
            action={{ label: "Create the first lead", onClick: () => {} }}
          />
        </Panel>
      </div>
    </Specimen>
  );
}

function Listing() {
  const [offset, setOffset] = useState(50);
  const [limit, setLimit] = useState(50);
  return (
    <Specimen
      title="Pager, Stat, CodeBlock"
      note="A list that pages says where you are (1–50 of 1,234) and keeps the pager in view. A stat is one big number with a label. Code is always a block with a copy button, highlighted from tokens, never from markup."
      wide
      props={[
        [
          "Pager offset / limit / total",
          "number",
          "required",
          "Zero-based offset of the first row, page size and the number of matches before paging.",
        ],
        [
          "Pager sizes",
          "number[]",
          "[25, 50, 100, 200]",
          "The choices of the rows-per-page select.",
        ],
        [
          "Stat label / value / hint",
          "string, ReactNode, ReactNode",
          "required, required, none",
          "A KPI tile. Numbers are tabular; money is formatted by the caller.",
        ],
        [
          "CodeBlock code / lang",
          "string, string",
          "required, plain text",
          "Highlights toml, json and sh. The copy button announces “Copied”.",
        ],
      ]}
    >
      <div className="data-grid">
        <Panel className="demo-pad" style={{ alignSelf: "start" }}>
          <Pager
            offset={offset}
            limit={limit}
            total={1234}
            onOffset={setOffset}
            onSize={(n) => {
              setLimit(n);
              setOffset(0);
            }}
          />
        </Panel>
        <Panel className="demo-pad stat-demo" style={{ alignSelf: "start" }}>
          <Stat label="Open leads" value="131" hint="Records" />
          <Stat label="Expected revenue" value="€1,069,375.00" hint="Sum of expected revenue" />
        </Panel>
        <CodeBlock
          lang="toml"
          code={`[[model.action]]\nname = "post"\nfield = "state"\nfrom = "draft"\nto = "posted"\napproval = true`}
        />
      </div>
    </Specimen>
  );
}

function TwoFactor() {
  return (
    <Specimen
      title="QrCode, TwoFactorStep"
      note="A QR code is drawn in the browser as one SVG path and is always dark on white with a quiet zone, in every theme, because scanners need that contrast; the typed key goes next to it for everyone who cannot scan. The second sign-in step asks for one thing at a time: the six digits, or a recovery code."
      wide
      props={[
        [
          "QrCode value / label",
          "string, string",
          "required",
          "The text to encode (an otpauth:// URI) and its accessible name. Error correction level M.",
        ],
        [
          "QrCode size",
          "number",
          "192",
          "Edge length in CSS pixels; it shrinks to fit its container.",
        ],
        [
          "TwoFactorStep onSubmit",
          "(factor) => Promise<string | null>",
          "required",
          "Resolves to the error to show, or null when the sign-in went through.",
        ],
        ["TwoFactorStep onBack", "() => void", "required", "Returns to the password step."],
      ]}
    >
      <div className="data-grid">
        <Panel className="demo-pad" style={{ alignSelf: "start" }}>
          <QrCode
            value="otpauth://totp/obop:ada%40example.test?secret=JBSWY3DPEHPK3PXP&issuer=obop"
            label="Example QR code"
            size={160}
          />
        </Panel>
        <Panel className="demo-pad" style={{ alignSelf: "start" }}>
          <TwoFactorStep onSubmit={async () => "That code is not valid."} onBack={() => {}} />
        </Panel>
      </div>
    </Specimen>
  );
}

export function Components({ notify }: { notify: Notify }) {
  return (
    <>
      <Block
        id="components"
        title="Components"
        lead="Everything in @obop/ui, in every state that matters. Hover and focus are forced here so you can compare them side by side; in the product they react to the pointer and keyboard. Open Props under a component for its API."
      >
        <div className="spec-stack">
          <Buttons />
          <IconButtons />
          <Fields />
          <Toggles />
          <Labels />
          <Surfaces />
          <Navigation />
          <Overlays notify={notify} />
          <Feedback notify={notify} />
          <DataDisplay />
          <Listing />
          <TwoFactor />
        </div>
      </Block>
    </>
  );
}
