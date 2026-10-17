import { useEffect, useMemo, useRef, useState } from "react";
import { Badge, t } from "@obop/ui";
import { call } from "../api.ts";
import { useData, useRows } from "../data.tsx";
import { serverMessage } from "../serverMessage.ts";
import { CodeBlock, Notice, Select, Skeleton } from "../ui.tsx";
import { className, parseJson, treatmentLabel } from "./model.ts";

type Treatments = {
  fields: {
    field: string;
    class: string;
    personal: boolean;
    treatment: string;
    why: string;
    denied: boolean;
    raised: boolean;
  }[];
  unknown: string[];
  classes: string[];
  sends_personal_data: boolean;
  sample: string | null;
};

export type Preview = { payload: string; treatments: Treatments };

type Props = {
  provider?: string;
  connection?: string;
  model: string;
  allow: Record<string, string[]>;
  policy: string;
  inbound?: boolean;
  /** Told whenever a preview arrives, so a step can show the classes that would flow. */
  onPreview?: (p: Preview | null) => void;
};

const why = (w: string) =>
  w === "default"
    ? t("default policy")
    : w === "class override"
      ? t("class raised by you")
      : w === "field override"
        ? t("field setting")
        : w === "secret never leaves"
          ? t("secrets never leave")
          : w === "class not allowed for this provider"
            ? t("class not allowed for this provider")
            : t("high-risk class without an acknowledgement");

/**
 * A real sample record as it would leave, after the policy: the exact payload, and for every
 * field what happens to it. Nothing is stored by looking.
 */
export function PrivacyPreview(p: Props) {
  const d = useData();
  const records = useRows(p.model);
  const [recordId, setRecordId] = useState("");
  const [state, setState] = useState<{ preview?: Preview; error?: string; busy: boolean }>({
    busy: true,
  });
  const seq = useRef(0);
  const allowText = useMemo(() => JSON.stringify(p.allow), [p.allow]);
  const { onPreview } = p;

  useEffect(() => {
    if (!p.model || !(p.allow[p.model] ?? []).length) {
      setState({ busy: false });
      onPreview?.(null);
      return;
    }
    const n = ++seq.current;
    setState((s) => ({ ...s, busy: true }));
    const timer = setTimeout(async () => {
      const r = await call(d.token, "privacy_preview.render", {
        ...(p.connection ? { connection: p.connection } : { provider: p.provider }),
        target_model: p.model,
        record_id: recordId,
        field_allowlist: allowText,
        policy: p.policy,
      });
      if (n !== seq.current) return;
      if (r.kind === "ok") {
        const v = r.value as { payload: string; treatments: string };
        const preview = {
          payload: v.payload,
          treatments: parseJson<Treatments>(v.treatments, null as never),
        };
        setState({ preview, busy: false });
        onPreview?.(preview);
      } else {
        setState({
          busy: false,
          error: r.kind === "error" ? serverMessage(r.message) : t("Needs approval"),
        });
        onPreview?.(null);
      }
    }, 250);
    return () => clearTimeout(timer);
  }, [d.token, p.provider, p.connection, p.model, allowText, p.policy, recordId, onPreview]);

  const tr = state.preview?.treatments;
  const rows = records.rows.slice(0, 25);
  return (
    <section className="cx-preview" aria-label={t("Privacy preview")} aria-live="polite">
      <h3>{t("Privacy preview")}</h3>
      <p className="hint">
        {t(
          "This is what one real record looks like after masking and pseudonymisation. It is computed by the same policy that runs when data leaves, and nothing is sent or stored by looking.",
        )}
      </p>
      {rows.length > 0 && (
        <Select
          label={t("Sample record")}
          value={recordId}
          onChange={(e) => setRecordId(e.target.value)}
        >
          <option value="">{t("The first record")}</option>
          {rows.map((r) => (
            <option key={r.id} value={r.id}>
              {d.recordLabel(p.model, r)}
            </option>
          ))}
        </Select>
      )}
      {state.error && (
        <Notice tone="warning" title={t("No preview yet")}>
          {state.error}
        </Notice>
      )}
      {!state.error && state.busy && !state.preview && <Skeleton rows={4} cols={3} />}
      {tr && (
        <>
          <table className="table dense cx-treatments">
            <caption className="sr-only">{t("What happens to each field")}</caption>
            <thead>
              <tr>
                <th scope="col">{t("Field")}</th>
                <th scope="col">{t("Data class")}</th>
                <th scope="col">{t("What happens")}</th>
              </tr>
            </thead>
            <tbody>
              {tr.fields.map((f) => (
                <tr key={f.field} data-treatment={f.treatment}>
                  <th scope="row">{f.field}</th>
                  <td>{className(f.class)}</td>
                  <td>
                    <Badge
                      tone={f.denied ? "danger" : f.treatment === "allow" ? "warning" : "info"}
                    >
                      {treatmentLabel(f.treatment, !!p.inbound)}
                    </Badge>{" "}
                    <span className="hint">{why(f.why)}</span>
                  </td>
                </tr>
              ))}
              {tr.unknown.map((u) => (
                <tr key={u}>
                  <th scope="row">{u}</th>
                  <td colSpan={2}>{t("Not a field of this model: ignored")}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {state.preview?.payload && state.preview.payload !== "null" ? (
            <>
              <h4>{t("The payload")}</h4>
              <CodeBlock code={state.preview.payload} lang="json" />
            </>
          ) : (
            <p className="hint">
              {t("There is no record to show yet: create one and look again.")}
            </p>
          )}
        </>
      )}
    </section>
  );
}
