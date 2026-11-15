// The Point of Sale register (UI plan 3.3): open or resume a session, ring up products from a grid
// or a scanned code, take cash or card (or both), print a receipt, start the next sale, and close
// the session with a cash count. Touch first. Everything goes through the package's own commands
// (pos_session.create, update and close; pos_order.create, pay and cancel; pos_order_line.create;
// pos_payment.create), so the CLI, REST and MCP can do every step too (OR-4).
//
// This is not a certified fiscal till: there is no technical security device (TSE), which live use
// in Germany needs (owner decision O4). The register says so and claims nothing else.
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Badge,
  Button,
  Dialog,
  EmptyState,
  Icon,
  IconButton,
  Input,
  Notice,
  Segmented,
  Skeleton,
  Textarea,
  compareText,
  t,
} from "@obop/ui";
import { call, search, type Reply, type Row } from "../../api.ts";
import { BlobThumb } from "../../BlobFields.tsx";
import type { App } from "../../Chrome.tsx";
import { useData, useRows } from "../../data.tsx";
import { formatDate, majorString, money, parseMoney } from "../../format.ts";
import { useMedia } from "../../media.ts";
import type { Go } from "../../route.ts";
import { serverMessage } from "../../serverMessage.ts";
import { PrintSheet } from "../sales/PrintSheet.tsx";
import {
  addProduct,
  byCode,
  cartTotal,
  cashTender,
  itemCount,
  lineTotal,
  matches,
  paidSoFar,
  percentToBp,
  quickCash,
  remaining,
  setDiscount,
  setQty,
  type CartLine,
  type Method,
  type Tender,
} from "./cart.ts";
import "./pos.css";

const SESSION = "pos_session";
const CONFIG = "pos_config";
const PRODUCT = "product";
/** Which open session this browser tab works in (a per-viewer convenience). */
const PICKED = "pos-register-session";
const CART = "pos-register-cart:";

const NARROW = "(max-width: 64rem)";

function readStore(key: string): string | null {
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}
function writeStore(key: string, value: string | null) {
  try {
    if (value === null) sessionStorage.removeItem(key);
    else sessionStorage.setItem(key, value);
  } catch {
    /* private window: the cart simply is not kept */
  }
}

const str = (v: unknown) => (typeof v === "string" ? v : "");
const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

/** The text of a reply that is not a success, for a toast. */
function failure(r: Reply): string {
  if (r.kind === "error") return serverMessage(r.message);
  return t("Sent for approval");
}

/** The value of a successful reply as a record. */
function rowOf(r: Reply): Row | null {
  return r.kind === "ok" && r.value && typeof r.value === "object" ? (r.value as Row) : null;
}

export function PosRegister({ app, go }: { app: App; go: Go }) {
  const sessions = useRows(SESSION);
  const configs = useRows(CONFIG);
  const [picked, setPicked] = useState<string | null>(() => readStore(PICKED));
  const open = sessions.rows.filter((s) => s.state === "open");
  const session = open.find((s) => s.id === picked) ?? (open.length === 1 ? open[0] : undefined);
  const config = session ? configs.rows.find((c) => c.id === session.config_id) : undefined;

  const choose = (id: string | null) => {
    writeStore(PICKED, id);
    setPicked(id);
  };

  if (!sessions.loaded || !configs.loaded)
    return (
      <section className="main panel pos pos-loading" aria-label={t("Register")}>
        <Skeleton rows={6} cols={3} />
      </section>
    );
  if (sessions.error || configs.error)
    return (
      <section className="main panel pos pos-center" aria-label={t("Register")}>
        <Notice tone="danger">{serverMessage(sessions.error ?? configs.error)}</Notice>
      </section>
    );
  if (!session || (open.length > 1 && !open.some((s) => s.id === picked)))
    return (
      <OpenRegister
        app={app}
        go={go}
        configs={configs.rows}
        open={open}
        onOpened={(id) => choose(id)}
      />
    );
  return (
    <Terminal
      key={session.id}
      session={session}
      config={config}
      onLeave={() => choose(null)}
      go={go}
      app={app}
      canSwitch={open.length > 1}
    />
  );
}

/** No session to work in yet: resume an open one, or open a till with its opening cash. */
function OpenRegister(p: {
  app: App;
  go: Go;
  configs: Row[];
  open: Row[];
  onOpened: (id: string) => void;
}) {
  const d = useData();
  const tills = useMemo(
    () =>
      p.configs
        .filter((c) => c.active !== false)
        .sort((a, b) => compareText(str(a.name), str(b.name))),
    [p.configs, d.lang],
  );
  const busyTills = new Set(p.open.map((s) => str(s.config_id)));
  const free = tills.filter((c) => !busyTills.has(c.id));
  const [till, setTill] = useState<string>(free[0]?.id ?? "");
  const chosen = tills.find((c) => c.id === till);
  const cur = str(chosen?.currency) || undefined;
  const [cash, setCash] = useState<string>("");
  const [busy, setBusy] = useState(false);
  // The drawer holds what was counted when the till last closed; the till's default only when it
  // never closed.
  useEffect(() => {
    let live = true;
    setCash(chosen ? majorString(num(chosen.default_opening_cash), cur) : "");
    if (chosen)
      void search(d.token, SESSION, {
        filter: [
          { field: "config_id", op: "eq", value: chosen.id },
          { field: "state", op: "eq", value: "closed" },
        ],
        order: [{ field: "closed_on", dir: "desc" }],
        limit: 1,
      }).then((r) => {
        const last = "error" in r ? undefined : r.rows[0];
        if (live && last && typeof last.closing_cash === "number")
          setCash(majorString(last.closing_cash, cur));
      });
    return () => {
      live = false;
    };
  }, [chosen, cur, d.token]);
  const minor = parseMoney(cash, cur);

  const openIt = async () => {
    if (!chosen || minor === null) return;
    setBusy(true);
    const r = await d.run(`${SESSION}.create`, { config_id: chosen.id, opening_cash: minor });
    setBusy(false);
    const row = rowOf(r);
    if (!row) return d.notify(failure(r), "error");
    d.notify(t("Register open"), "ok");
    p.onOpened(row.id);
  };

  if (!tills.length)
    return (
      <section className="main panel pos pos-center" aria-label={t("Register")}>
        <EmptyState
          icon="pos"
          title={t("Set up a till first")}
          hint={t("A till holds the currency, the opening cash and the receipt text.")}
          action={{
            label: t("Add a till"),
            onClick: () => p.go({ app: p.app.id, model: CONFIG, id: "new" }),
          }}
        />
      </section>
    );

  return (
    <section className="main panel pos pos-center" aria-label={t("Register")}>
      <section className="pos-open" aria-labelledby="pos-open-title">
        <div className="pos-open-art" aria-hidden="true">
          <Icon name="pos" size={28} />
        </div>
        <h1 id="pos-open-title">{t("Open the register")}</h1>
        {p.open.length > 0 && (
          <div className="pos-resume">
            <h2>{t("Continue a session")}</h2>
            {p.open.map((s) => (
              <button
                key={s.id}
                type="button"
                className="pos-choice"
                onClick={() => p.onOpened(s.id)}
              >
                <span className="pos-choice-main">
                  <strong>{d.refText(CONFIG, str(s.config_id))}</strong>
                  <span>
                    {t("Opened {date} by {cashier}", {
                      date: formatDate(s.opened_on),
                      cashier: str(s.cashier),
                    })}
                  </span>
                </span>
                <Icon name="chevron-right" size={18} />
              </button>
            ))}
          </div>
        )}
        {free.length > 0 && (
          <div className="pos-new">
            {p.open.length > 0 && <h2>{t("Or open another till")}</h2>}
            {free.length > 1 && (
              <div className="pos-tills" role="radiogroup" aria-label={t("Till")}>
                {free.map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    role="radio"
                    aria-checked={c.id === till}
                    className="pos-choice"
                    onClick={() => setTill(c.id)}
                  >
                    <span className="pos-choice-main">
                      <strong>{str(c.name)}</strong>
                    </span>
                    {c.id === till && <Icon name="check" size={18} />}
                  </button>
                ))}
              </div>
            )}
            {free.length === 1 && chosen && <p className="pos-till-one">{str(chosen.name)}</p>}
            <Input
              label={t("Cash in the drawer")}
              inputMode="decimal"
              value={cash}
              onChange={(e) => setCash(e.target.value)}
              error={cash && minor === null ? t("Enter an amount") : undefined}
              hint={t("Count the drawer before the first sale.")}
            />
            <Button
              variant="primary"
              size="lg"
              className="pos-wide"
              loading={busy}
              disabled={!chosen || minor === null}
              onClick={() => void openIt()}
            >
              {t("Open register")}
            </Button>
          </div>
        )}
        <FiscalNote />
      </section>
    </section>
  );
}

function FiscalNote() {
  return (
    <p className="pos-fiscal">
      <Badge tone="warning">{t("Experimental")}</Badge>
      <span>
        {t(
          "This register records sales and payments. It is not a certified fiscal till: it has no technical security device (TSE).",
        )}
      </span>
    </p>
  );
}

type Done = {
  order: Row;
  lines: CartLine[];
  tenders: Tender[];
  change: number;
};

function Terminal(p: {
  session: Row;
  config: Row | undefined;
  onLeave: () => void;
  go: Go;
  app: App;
  canSwitch: boolean;
}) {
  const d = useData();
  const products = useRows(PRODUCT);
  const narrow = useMedia(NARROW);
  const cur = str(p.config?.currency) || undefined;
  const cartKey = CART + p.session.id;
  const [cart, setCartState] = useState<CartLine[]>(() => {
    try {
      const v = JSON.parse(readStore(cartKey) ?? "[]") as CartLine[];
      return Array.isArray(v) ? v : [];
    } catch {
      return [];
    }
  });
  const setCart = (next: CartLine[]) => {
    setCartState(next);
    writeStore(cartKey, next.length ? JSON.stringify(next) : null);
  };
  const [q, setQ] = useState("");
  const [cartOpen, setCartOpen] = useState(false);
  const [paying, setPaying] = useState(false);
  const [closing, setClosing] = useState(false);
  const [done, setDone] = useState<Done | null>(null);
  const [added, setAdded] = useState<string | null>(null);
  const search = useRef<HTMLInputElement>(null);

  const list = useMemo(
    () =>
      products.rows
        .map((r) => ({
          id: r.id,
          name: str(r.name),
          sku: str(r.sku) || undefined,
          price: num(r.list_price_minor),
          image: str(r.image),
        }))
        .sort((a, b) => compareText(a.name, b.name)),
    [products.rows, d.lang],
  );
  const shown = useMemo(() => list.filter((x) => matches(x, q)), [list, q]);
  const total = cartTotal(cart);
  const count = itemCount(cart);

  const add = (x: { id: string; name: string; price: number }) => {
    setCart(addProduct(cart, x));
    setAdded(x.id);
    window.setTimeout(() => setAdded((v) => (v === x.id ? null : v)), 600);
  };

  const onSearchKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    const hit = byCode(list, q) ?? (shown.length === 1 ? shown[0] : undefined);
    if (hit) {
      add(hit);
      setQ("");
    }
  };

  const title = p.config ? str(p.config.name) : d.refText(SESSION, p.session.id);

  const cartPanel = (
    <CartPanel
      lines={cart}
      currency={cur}
      onChange={setCart}
      onPay={() => {
        setCartOpen(false);
        setPaying(true);
      }}
    />
  );

  return (
    <section className="main panel pos" aria-label={t("Register")}>
      <header className="pos-bar">
        <div className="pos-bar-title">
          <h1>{title}</h1>
          <p>
            <span>
              {t("Opened {date} by {cashier}", {
                date: formatDate(p.session.opened_on),
                cashier: str(p.session.cashier),
              })}
            </span>
            {/* Always on the till: this register is not a certified fiscal till (no TSE). */}
            <span
              className="pos-bar-fiscal"
              title={t(
                "This register records sales and payments. It is not a certified fiscal till: it has no technical security device (TSE).",
              )}
            >
              <Badge tone="warning">{t("Not for fiscal use")}</Badge>
            </span>
          </p>
        </div>
        <div className="pos-bar-actions">
          {p.canSwitch && (
            <Button size="sm" variant="ghost" onClick={p.onLeave}>
              {t("Switch till")}
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            icon="list"
            onClick={() => p.go({ app: p.app.id, model: "pos_order" })}
          >
            {t("Sales")}
          </Button>
          <Button size="sm" icon="lock" onClick={() => setClosing(true)}>
            {t("Close register")}
          </Button>
        </div>
      </header>
      <div className="pos-main">
        <section className="pos-catalog" aria-label={t("Products")}>
          <div className="pos-search">
            <Icon name="search" size={18} />
            <input
              ref={search}
              type="search"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              onKeyDown={onSearchKey}
              placeholder={t("Search products or scan a code")}
              aria-label={t("Search products or scan a code")}
              enterKeyHint="go"
              autoComplete="off"
            />
          </div>
          {!products.loaded ? (
            <Skeleton rows={4} cols={3} />
          ) : products.error ? (
            <Notice tone="danger">{serverMessage(products.error)}</Notice>
          ) : shown.length === 0 ? (
            <EmptyState
              icon="search"
              title={q ? t("No product matches") : t("No products yet")}
              hint={q ? t("Try another word or code.") : undefined}
            />
          ) : (
            <ul className="pos-grid">
              {shown.map((x) => {
                const inCart = cart
                  .filter((l) => l.product === x.id)
                  .reduce((s, l) => s + l.qty, 0);
                return (
                  <li key={x.id}>
                    <button
                      type="button"
                      className="pos-tile"
                      data-added={added === x.id || undefined}
                      onClick={() => add(x)}
                      aria-label={t("Add {product}, {price}", {
                        product: x.name,
                        price: money(x.price, cur),
                      })}
                    >
                      <span className="pos-tile-img" aria-hidden="true">
                        {x.image ? (
                          <BlobThumb id={x.image} alt="" size={88} />
                        ) : (
                          <span className="pos-tile-ph">{x.name.slice(0, 2)}</span>
                        )}
                      </span>
                      <span className="pos-tile-name">{x.name}</span>
                      <span className="pos-tile-price">{money(x.price, cur)}</span>
                      {inCart > 0 && (
                        <span className="pos-tile-count" aria-hidden="true">
                          {inCart}
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
        {!narrow && <aside className="pos-cart">{cartPanel}</aside>}
      </div>
      {narrow && (
        <div className="pos-dock">
          <button
            type="button"
            className="pos-dock-btn"
            disabled={!cart.length}
            onClick={() => setCartOpen(true)}
          >
            <span className="pos-dock-count">
              {count === 1 ? t("1 item") : t("{n} items", { n: count })}
            </span>
            <span className="pos-dock-total">{money(total, cur)}</span>
            <span className="pos-dock-go">
              {t("Review")} <Icon name="chevron-up" size={16} />
            </span>
          </button>
        </div>
      )}
      {narrow && cartOpen && (
        <Dialog
          open
          onClose={() => setCartOpen(false)}
          title={t("Current sale")}
          variant="sheet"
          className="pos-sheet"
        >
          {cartPanel}
        </Dialog>
      )}
      {paying && (
        <PaySheet
          total={total}
          currency={cur}
          session={p.session}
          lines={cart}
          onClose={() => setPaying(false)}
          onDone={(x) => {
            setPaying(false);
            setCart([]);
            setDone(x);
          }}
        />
      )}
      {done && (
        <Receipt
          done={done}
          config={p.config}
          currency={cur}
          onNext={() => {
            setDone(null);
            if (!narrow) search.current?.focus();
          }}
        />
      )}
      {closing && (
        <CloseDialog
          session={p.session}
          currency={cur}
          onClose={() => setClosing(false)}
          onClosed={() => {
            setClosing(false);
            writeStore(cartKey, null);
            p.onLeave();
          }}
        />
      )}
    </section>
  );
}

/** The lines of the sale with quantity steppers, a discount per line, the total and Pay. */
function CartPanel(p: {
  lines: CartLine[];
  currency?: string;
  onChange: (l: CartLine[]) => void;
  onPay: () => void;
}) {
  const [editing, setEditing] = useState<number | null>(null);
  const [pct, setPct] = useState("");
  const total = cartTotal(p.lines);
  const bp = percentToBp(pct);
  if (!p.lines.length)
    return (
      <div className="pos-cart-empty">
        <Icon name="pos" size={28} />
        <p>{t("Tap a product to start a sale.")}</p>
      </div>
    );
  return (
    <div className="pos-cart-body">
      <ol className="pos-lines">
        {p.lines.map((l, i) => (
          <li key={`${l.product}-${i}`} className="pos-line">
            <div className="pos-line-top">
              <button
                type="button"
                className="pos-line-name"
                aria-expanded={editing === i}
                onClick={() => {
                  setEditing(editing === i ? null : i);
                  setPct(l.discountBp ? String(l.discountBp / 100) : "");
                }}
              >
                <span>{l.name}</span>
                <span className="pos-line-unit">
                  {money(l.price, p.currency)}
                  {l.discountBp > 0 &&
                    ` · ${t("{pct} % off", { pct: (l.discountBp / 100).toLocaleString() })}`}
                </span>
              </button>
              <span className="pos-line-total">{money(lineTotal(l), p.currency)}</span>
            </div>
            <div className="pos-line-qty">
              <IconButton
                icon={l.qty === 1 ? "trash" : "minus"}
                label={l.qty === 1 ? t("Remove {product}", { product: l.name }) : t("One less")}
                tooltip={false}
                onClick={() => p.onChange(setQty(p.lines, i, l.qty - 1))}
              />
              <span className="pos-qty" aria-live="polite">
                {l.qty}
              </span>
              <IconButton
                icon="plus"
                label={t("One more")}
                tooltip={false}
                onClick={() => p.onChange(setQty(p.lines, i, l.qty + 1))}
              />
              <button
                type="button"
                className="pos-line-disc"
                aria-expanded={editing === i}
                aria-label={t("Discount on {product}", { product: l.name })}
                onClick={() => {
                  setEditing(editing === i ? null : i);
                  setPct(l.discountBp ? String(l.discountBp / 100) : "");
                }}
              >
                %
              </button>
            </div>
            {editing === i && (
              <form
                className="pos-line-edit"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (bp === null) return;
                  p.onChange(setDiscount(p.lines, i, bp));
                  setEditing(null);
                }}
              >
                <Input
                  label={t("Discount (%)")}
                  inputMode="decimal"
                  value={pct}
                  autoFocus
                  onChange={(e) => setPct(e.target.value)}
                  error={bp === null ? t("Enter a percentage from 0 to 100") : undefined}
                />
                <Button type="submit" disabled={bp === null}>
                  {t("Apply")}
                </Button>
              </form>
            )}
          </li>
        ))}
      </ol>
      <div className="pos-cart-foot">
        <div className="pos-total">
          <span>{t("Total")}</span>
          <strong>{money(total, p.currency)}</strong>
        </div>
        <Button variant="primary" size="lg" className="pos-wide pos-pay" onClick={p.onPay}>
          {t("Pay {amount}", { amount: money(total, p.currency) })}
        </Button>
        <Button variant="ghost" className="pos-wide" onClick={() => p.onChange([])}>
          {t("Clear sale")}
        </Button>
      </div>
    </div>
  );
}

const METHODS: { value: Method; label: () => string }[] = [
  { value: "cash", label: () => t("Cash") },
  { value: "card", label: () => t("Card") },
  { value: "voucher", label: () => t("Voucher") },
];

/**
 * Take the money: one or more payments until the total is covered, then the sale is written as an
 * order with its lines and payments and `pay` is run (which the server refuses until the payments
 * cover the total).
 */
function PaySheet(p: {
  total: number;
  currency?: string;
  session: Row;
  lines: CartLine[];
  onClose: () => void;
  onDone: (d: Done) => void;
}) {
  const d = useData();
  const [tenders, setTenders] = useState<Tender[]>([]);
  const [method, setMethod] = useState<Method>("cash");
  const owed = remaining(p.total, tenders);
  const [given, setGiven] = useState(() => majorString(owed, p.currency));
  const [ref, setRef] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => setGiven(majorString(owed, p.currency)), [owed, p.currency]);
  const amount = parseMoney(given, p.currency);
  const valid = amount !== null && amount > 0 && (method === "cash" || amount <= owed);
  const cash = method === "cash" && amount !== null ? cashTender(amount, owed) : null;
  const covers = amount !== null && amount >= owed;

  const tenderNow = (): { all: Tender[]; change: number } | null => {
    if (amount === null || amount <= 0) return null;
    if (method === "cash") {
      const c = cashTender(amount, owed);
      return { all: [...tenders, { method, amount: c.amount }], change: c.change };
    }
    const x: Tender = { method, amount: Math.min(amount, owed) };
    if (ref.trim()) x.reference = ref.trim();
    return { all: [...tenders, x], change: 0 };
  };

  const finish = async (all: Tender[], change: number) => {
    setBusy(true);
    try {
      const made = await d.run("pos_order.create", { session_id: p.session.id });
      const order = rowOf(made);
      if (!order) return d.notify(failure(made), "error");
      const undo = async (why: string) => {
        // The receipt number is gapless, so a sale that could not be completed is cancelled, not
        // deleted: the number stays accounted for.
        await d.run("pos_order.cancel", { id: order.id });
        d.notify(why, "error");
      };
      for (const l of p.lines) {
        const r = await d.run("pos_order_line.create", {
          order_id: order.id,
          product_id: l.product,
          quantity: l.qty,
          discount_bp: l.discountBp,
        });
        if (r.kind !== "ok") return undo(failure(r));
      }
      // The server's total is the authority; the last payment takes up any difference.
      const fresh = rowOf(await call(d.token, "pos_order.get", { id: order.id }));
      const serverTotal = num(fresh?.total);
      const paid = all.slice(0, -1);
      const last = all[all.length - 1]!;
      const rest = Math.max(0, serverTotal - paidSoFar(paid));
      const final = [...paid, { ...last, amount: rest }].filter((x) => x.amount > 0);
      for (const x of final) {
        const body: Record<string, unknown> = {
          order_id: order.id,
          method: x.method,
          amount: x.amount,
        };
        if (x.reference) body.provider_reference = x.reference;
        const r = await d.run("pos_payment.create", body);
        if (r.kind !== "ok") return undo(failure(r));
      }
      const paidReply = await d.run("pos_order.pay", { id: order.id });
      if (paidReply.kind !== "ok") return undo(failure(paidReply));
      const after = rowOf(await call(d.token, "pos_order.get", { id: order.id })) ?? order;
      p.onDone({ order: after, lines: p.lines, tenders: final, change });
    } finally {
      setBusy(false);
    }
  };

  const primary = () => {
    const n = tenderNow();
    if (!n) return;
    if (paidSoFar(n.all) >= p.total) void finish(n.all, n.change);
    else {
      setTenders(n.all);
      setRef("");
    }
  };

  return (
    <Dialog
      open
      onClose={() => !busy && p.onClose()}
      title={t("Payment")}
      className="pos-paysheet"
      footer={
        <Button
          variant="primary"
          size="lg"
          className="pos-wide"
          loading={busy}
          disabled={!valid}
          onClick={primary}
        >
          {covers
            ? t("Complete sale")
            : t("Add {amount} by {method}", {
                amount: money(amount ?? 0, p.currency),
                method: METHODS.find((m) => m.value === method)!
                  .label()
                  .toLowerCase(),
              })}
        </Button>
      }
    >
      <div className="pos-due">
        <span>{tenders.length ? t("Still to pay") : t("To pay")}</span>
        <strong>{money(owed, p.currency)}</strong>
        {tenders.length > 0 && (
          <span className="pos-due-of">
            {t("of {total}", { total: money(p.total, p.currency) })}
          </span>
        )}
      </div>
      {tenders.length > 0 && (
        <ul className="pos-tenders">
          {tenders.map((x, i) => (
            <li key={i}>
              <span>{METHODS.find((m) => m.value === x.method)?.label() ?? x.method}</span>
              <span className="tabular">{money(x.amount, p.currency)}</span>
              <IconButton
                icon="close"
                size="sm"
                label={t("Remove this payment")}
                tooltip={false}
                onClick={() => setTenders(tenders.filter((_, j) => j !== i))}
              />
            </li>
          ))}
        </ul>
      )}
      <Segmented
        label={t("Payment method")}
        options={METHODS.map((m) => ({
          value: m.value,
          label: m.label(),
          icon: m.value === "cash" ? "pos" : m.value === "card" ? "key" : "star",
        }))}
        value={method}
        onChange={(m) => {
          setMethod(m);
          // A card or a voucher pays what is owed; cash starts from it too.
          setGiven(majorString(owed, p.currency));
        }}
      />
      <Input
        label={method === "cash" ? t("Cash received") : t("Amount")}
        inputMode="decimal"
        value={given}
        onChange={(e) => setGiven(e.target.value)}
        error={
          given && amount === null
            ? t("Enter an amount")
            : method !== "cash" && amount !== null && amount > owed
              ? t("A card or voucher cannot pay more than is owed")
              : undefined
        }
      />
      {method === "cash" && (
        <div className="pos-quick" role="group" aria-label={t("Amount received")}>
          {quickCash(owed).map((v) => (
            <Button key={v} onClick={() => setGiven(majorString(v, p.currency))}>
              {money(v, p.currency)}
            </Button>
          ))}
        </div>
      )}
      {method !== "cash" && (
        <Input
          label={t("Terminal reference (optional)")}
          hint={t("Never a card number.")}
          value={ref}
          onChange={(e) => setRef(e.target.value)}
          autoComplete="off"
        />
      )}
      {cash && cash.change > 0 && (
        <div className="pos-change" aria-live="polite">
          <span>{t("Change")}</span>
          <strong>{money(cash.change, p.currency)}</strong>
        </div>
      )}
    </Dialog>
  );
}

/** The paid sale: what to hand over, Print, and the next sale as the one primary action. */
function Receipt(p: {
  done: Done;
  config: Row | undefined;
  currency?: string;
  onNext: () => void;
}) {
  const { change } = p.done;
  return (
    <Dialog
      open
      onClose={p.onNext}
      title={t("Sale complete")}
      className="pos-receipt-dialog"
      footer={
        <>
          <Button icon="printer" onClick={() => window.print()}>
            {t("Print receipt")}
          </Button>
          <Button variant="primary" size="lg" autoFocus onClick={p.onNext}>
            {t("New sale")}
          </Button>
        </>
      }
    >
      {change > 0 && (
        <div className="pos-change pos-change-big" role="status">
          <span>{t("Give change")}</span>
          <strong>{money(change, p.currency)}</strong>
        </div>
      )}
      <article className="pos-receipt" aria-label={t("Receipt")}>
        <ReceiptBody {...p} />
      </article>
      {/* The copy on paper: once, 80 mm wide, outside the dialog (PrintSheet). */}
      <PrintSheet>
        <article className="pos-receipt pos-receipt-paper">
          <ReceiptBody {...p} />
        </article>
      </PrintSheet>
    </Dialog>
  );
}

/** What a receipt says: the header, the number and day, the lines, the total and the payments. */
function ReceiptBody(p: { done: Done; config: Row | undefined; currency?: string }) {
  const { order, lines, tenders, change } = p.done;
  const header = str(p.config?.receipt_header);
  return (
    <>
      {header && <pre className="pos-receipt-head">{header}</pre>}
      <p className="pos-receipt-meta">
        <span>{str(order.receipt_number)}</span>
        <span>{formatDate(order.ordered_on)}</span>
      </p>
      <table>
        <tbody>
          {lines.map((l, i) => (
            <tr key={i}>
              <td>
                {l.qty} × {l.name}
                {l.discountBp > 0 && (
                  <span className="pos-receipt-sub">
                    {t("{pct} % off", { pct: (l.discountBp / 100).toLocaleString() })}
                  </span>
                )}
              </td>
              <td className="tabular">{money(lineTotal(l), p.currency)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr className="pos-receipt-total">
            <th scope="row">{t("Total")}</th>
            <td className="tabular">{money(num(order.total), p.currency)}</td>
          </tr>
          {tenders.map((x, i) => (
            <tr key={`t${i}`}>
              <th scope="row">{METHODS.find((m) => m.value === x.method)?.label() ?? x.method}</th>
              <td className="tabular">
                {money(x.amount + (x.method === "cash" ? change : 0), p.currency)}
              </td>
            </tr>
          ))}
          {change > 0 && (
            <tr>
              <th scope="row">{t("Change")}</th>
              <td className="tabular">{money(change, p.currency)}</td>
            </tr>
          )}
        </tfoot>
      </table>
    </>
  );
}

/** Count the drawer and close the session: the count first, then the close action on its own. */
function CloseDialog(p: {
  session: Row;
  currency?: string;
  onClose: () => void;
  onClosed: () => void;
}) {
  const d = useData();
  const opening = num(p.session.opening_cash);
  const [cashSales, setCashSales] = useState<number | null>(null);
  const [count, setCount] = useState(0);
  const [counted, setCounted] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    void (async () => {
      const orders = await call(d.token, "pos_order.search", {
        filter: [
          { field: "session_id", op: "eq", value: p.session.id },
          { field: "state", op: "eq", value: "paid" },
        ],
        limit: 5000,
      });
      const ids =
        orders.kind === "ok" ? (orders.value as { rows: Row[] }).rows.map((r) => r.id) : [];
      let cash = 0;
      if (ids.length) {
        const pays = await call(d.token, "pos_payment.search", {
          filter: [
            { field: "order_id", op: "in", value: ids },
            { field: "method", op: "eq", value: "cash" },
          ],
          limit: 5000,
        });
        if (pays.kind === "ok")
          cash = (pays.value as { rows: Row[] }).rows.reduce((s, r) => s + num(r.amount), 0);
      }
      if (live) {
        setCount(ids.length);
        setCashSales(cash);
      }
    })();
    return () => {
      live = false;
    };
  }, [d.token, p.session.id]);
  const expected = opening + (cashSales ?? 0);
  useEffect(() => {
    if (cashSales !== null) setCounted(majorString(expected, p.currency));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cashSales]);
  const closing = parseMoney(counted, p.currency);
  const diff = closing === null ? null : closing - expected;
  // A difference closes only with a note that explains it (the close action asserts the same).
  const [tried, setTried] = useState(false);
  const noteMissing = !!diff && !note.trim();

  const closeIt = async () => {
    if (closing === null) return;
    if (noteMissing) return setTried(true);
    setBusy(true);
    try {
      // The count is an update of its own; the state moves only through the close action.
      const body: Record<string, unknown> = {
        id: p.session.id,
        expected_cash: expected,
        closing_cash: closing,
        cash_difference: diff ?? 0,
      };
      if (note.trim()) body.note = note.trim();
      const u = await d.run(`${SESSION}.update`, body);
      if (u.kind !== "ok") return d.notify(failure(u), "error");
      const c = await d.run(`${SESSION}.close`, { id: p.session.id });
      if (c.kind === "approval") {
        d.notify(t("The count is saved. A manager approves the closing."), "info");
        return p.onClose();
      }
      if (c.kind !== "ok") return d.notify(failure(c), "error");
      d.notify(t("Register closed"), "ok");
      p.onClosed();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={() => !busy && p.onClose()}
      title={t("Close the register")}
      footer={
        <>
          <Button onClick={p.onClose} disabled={busy}>
            {t("Keep selling")}
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={closing === null || cashSales === null}
            onClick={() => void closeIt()}
          >
            {t("Close register")}
          </Button>
        </>
      }
    >
      {cashSales === null ? (
        <Skeleton rows={3} cols={2} />
      ) : (
        <>
          <dl className="pos-sum">
            <div>
              <dt>{t("Sales in this session")}</dt>
              <dd className="tabular">{count}</dd>
            </div>
            <div>
              <dt>{t("Opening cash")}</dt>
              <dd className="tabular">{money(opening, p.currency)}</dd>
            </div>
            <div>
              <dt>{t("Cash taken")}</dt>
              <dd className="tabular">{money(cashSales, p.currency)}</dd>
            </div>
            <div className="pos-sum-total">
              <dt>{t("Expected in the drawer")}</dt>
              <dd className="tabular">{money(expected, p.currency)}</dd>
            </div>
          </dl>
          <Input
            label={t("Cash counted")}
            inputMode="decimal"
            value={counted}
            onChange={(e) => setCounted(e.target.value)}
            error={counted && closing === null ? t("Enter an amount") : undefined}
          />
          {diff !== null && diff !== 0 && (
            <Notice tone={diff < 0 ? "danger" : "warning"}>
              {diff < 0
                ? t("{amount} is missing from the drawer.", {
                    amount: money(-diff, p.currency),
                  })
                : t("{amount} more than expected is in the drawer.", {
                    amount: money(diff, p.currency),
                  })}
            </Notice>
          )}
          <Textarea
            label={t("Note")}
            hint={diff && !(tried && noteMissing) ? t("Explain the difference.") : undefined}
            error={tried && noteMissing ? t("Explain the difference before closing.") : undefined}
            rows={2}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <p className="hint">{t("A closed session cannot be changed.")}</p>
        </>
      )}
    </Dialog>
  );
}
