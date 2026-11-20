import { useState } from "react";
import { Button, Notice, t } from "@obop/ui";
import { call } from "../api.ts";
import { serverMessage } from "../serverMessage.ts";

/** One row of `module.stored_limits` (K46). */
export type StoredLimit = {
  module: string;
  model: string;
  field: string;
  records: number;
  outside: number;
  longest: number | null;
  max_length: number | null;
  min_length: number | null;
  min: string | null;
  max: string | null;
  pattern: string | null;
};

/** What a field is held to, in words: "at most 2048 characters", "IBAN", "from 0 to 9". */
export function heldTo(r: StoredLimit): string {
  const parts: string[] = [];
  if (r.pattern === "iban") parts.push("IBAN");
  else if (r.pattern === "vat_id") parts.push(t("VAT number"));
  else if (r.pattern === "currency_code") parts.push(t("currency code"));
  else if (r.pattern) parts.push(r.pattern);
  if (r.min_length != null && r.max_length != null)
    parts.push(t("{min} to {max} characters", { min: r.min_length, max: r.max_length }));
  else if (r.max_length != null) parts.push(t("at most {max} characters", { max: r.max_length }));
  else if (r.min_length != null) parts.push(t("at least {min} characters", { min: r.min_length }));
  if (r.min != null && r.max != null)
    parts.push(t("from {min} to {max}", { min: r.min, max: r.max }));
  else if (r.max != null) parts.push(t("at most {max}", { max: r.max }));
  else if (r.min != null) parts.push(t("at least {min}", { min: r.min }));
  return parts.join(" · ");
}

/** The fields whose stored values do not fit, the worst first. */
export function outsideFirst(rows: StoredLimit[]): StoredLimit[] {
  return rows
    .filter((r) => r.outside > 0)
    .sort(
      (a, b) =>
        b.outside - a.outside || `${a.model}.${a.field}`.localeCompare(`${b.model}.${b.field}`),
    );
}

/**
 * Check stored values (K46): asks the server which stored values of this module are outside the
 * limits and formats enforced now (set or lowered after they were written). Read-only; a value
 * outside them stays readable, and only a write of that field has to fit.
 */
export function StoredLimits({ token, module }: { token: string; module: string }) {
  const [state, setState] = useState<
    | { kind: "idle" }
    | { kind: "busy" }
    | { kind: "done"; rows: StoredLimit[] }
    | { kind: "error"; message: string }
  >({ kind: "idle" });
  const run = async () => {
    setState({ kind: "busy" });
    const r = await call(token, "module.stored_limits", { id: module });
    if (r.kind === "ok") {
      const v = r.value as { stored_limits?: StoredLimit[] } | null;
      setState({ kind: "done", rows: v?.stored_limits ?? [] });
    } else if (r.kind === "error") setState({ kind: "error", message: serverMessage(r.message) });
    else setState({ kind: "error", message: t("The check needs an approval.") });
  };
  const bad = state.kind === "done" ? outsideFirst(state.rows) : [];
  return (
    <section className="stack" aria-labelledby="ct-stored-h">
      <h3 id="ct-stored-h" className="overline">
        {t("Stored values")}
      </h3>
      <p className="hint">
        {t(
          "A limit or format that was set or tightened after values were stored does not change them. Check which stored values no longer fit: they stay readable, and only a change of that field must fit.",
        )}
      </p>
      <div>
        <Button icon="search" onClick={run} disabled={state.kind === "busy"}>
          {state.kind === "busy" ? t("Checking…") : t("Check stored values")}
        </Button>
      </div>
      <div>
        {state.kind === "error" && (
          <Notice tone="danger" title={t("The check failed")}>
            {state.message}
          </Notice>
        )}
        {state.kind === "done" && bad.length === 0 && (
          <Notice tone="success" title={t("Every stored value fits")}>
            {t("{count, plural, one {# field was checked.} other {# fields were checked.}}", {
              count: state.rows.length,
            })}
          </Notice>
        )}
        {state.kind === "done" && bad.length > 0 && (
          <Notice tone="warning" title={t("Some stored values do not fit")}>
            {t(
              "{count, plural, one {# field holds values} other {# fields hold values}} outside the limits enforced now.",
              { count: bad.length },
            )}
          </Notice>
        )}
      </div>
      {bad.length > 0 && (
        <ul className="plain ct-models" aria-label={t("Stored values outside the limits")}>
          {bad.map((r) => (
            <li key={`${r.model}.${r.field}`}>
              <code>
                {r.model}.{r.field}
              </code>
              <p className="hint">{heldTo(r)}</p>
              <p>
                {t(
                  "{outside} of {records, plural, one {# record} other {# records}} outside the limits",
                  { outside: r.outside, records: r.records },
                )}
                {r.longest != null && (
                  <span className="hint">
                    {" · "}
                    {t("the longest has {count, plural, one {# character} other {# characters}}", {
                      count: r.longest,
                    })}
                  </span>
                )}
              </p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
