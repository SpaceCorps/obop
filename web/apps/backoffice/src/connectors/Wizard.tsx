import { useMemo, useState } from "react";
import { Badge, t } from "@obop/ui";
import { useData } from "../data.tsx";
import { modelTitle } from "../labels.ts";
import { serverMessage } from "../serverMessage.ts";
import { Button, Checkbox, Dialog, Input, Notice, Select, Textarea } from "../ui.tsx";
import { FieldPicker } from "./FieldPicker.tsx";
import { PrivacyPreview } from "./PrivacyPreview.tsx";
import {
  HIGH_RISK,
  blockedModel,
  adequacyLabel,
  basisLabel,
  className,
  configFields,
  defaultTreatment,
  directionLabel,
  maturityLabel,
  maturityTone,
  newSecret,
  parseJson,
  type Provider,
} from "./model.ts";

type Step = "provider" | "settings" | "fields" | "basis" | "connect";

type Props = {
  providers: Provider[];
  /** Provider to start from (the Connect button of a catalogue card). */
  initial?: string;
  isAdmin: boolean;
  onClose: () => void;
  /** Called when the wizard ends; `id` is the connection it made, if any. */
  onDone: (id?: string) => void;
};

const BASES = ["consent", "contract", "legitimate_interest", "legal_obligation"];

const stepLabel = (s: Step) =>
  s === "provider"
    ? t("Service")
    : s === "settings"
      ? t("Settings")
      : s === "fields"
        ? t("Fields")
        : s === "basis"
          ? t("Purpose and legal basis")
          : t("Test and connect");

export function Wizard(props: Props) {
  const d = useData();
  const startProvider = props.providers.find((p) => p.id === props.initial);
  const [step, setStep] = useState<Step>(startProvider ? "settings" : "provider");
  const [provider, setProvider] = useState<Provider | undefined>(startProvider);
  const [name, setName] = useState(startProvider?.name ?? "");
  const [values, setValues] = useState<Record<string, string>>({});
  const [secret, setSecret] = useState("");
  const [allow, setAllow] = useState<Record<string, string[]>>({});
  const [raised, setRaised] = useState<Record<string, string>>({});
  const [purpose, setPurpose] = useState("");
  const [basis, setBasis] = useState(startProvider?.legal_basis || "consent");
  const [note, setNote] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [connection, setConnection] = useState<string | null>(null);
  const [consentDone, setConsentDone] = useState(false);
  const [checks, setChecks] = useState<string[] | null>(null);
  const [previewModel, setPreviewModel] = useState("");

  const steps: Step[] = ["provider", "settings", "fields", "basis", "connect"];
  const cfg = useMemo(() => (provider ? configFields(provider) : []), [provider]);
  const inbound = provider?.direction === "inbound";
  const max = useMemo(() => (provider?.max_classes ?? "").split(",").filter(Boolean), [provider]);
  const target = values.target_model ?? "";
  const models = useMemo(() => [...d.models.values()], [d.models]);

  const choose = (p: Provider) => {
    setProvider(p);
    setName((n) => n || p.name);
    setBasis(p.legal_basis || "consent");
    setValues({});
    setSecret("");
    setAllow({});
    setRaised({});
  };

  // The classes of the chosen fields, and which of them stay withheld without an acknowledgement.
  const fieldClasses = useMemo(() => {
    const out = new Map<string, boolean>();
    for (const [model, names] of Object.entries(allow)) {
      const m = d.models.get(model)?.model;
      for (const n of names) {
        const f = m?.fields.find((x) => x.name === n);
        if (f) out.set(f.class, (out.get(f.class) ?? false) || f.personal_data);
      }
    }
    return out;
  }, [allow, d.models]);
  const highRisk = [...fieldClasses.keys()].filter((c) => HIGH_RISK.includes(c));
  const raisedClasses = Object.entries(raised)
    .filter(([c, text]) => max.includes(c) && text.trim().length >= 30)
    .map(([c]) => c);
  const policy = useMemo(
    () =>
      raisedClasses.length
        ? JSON.stringify({
            classes: Object.fromEntries(raisedClasses.map((c) => [c, "pseudonymise"])),
            acks: raisedClasses.map((c) => ({ class: c, text: raised[c]!.trim() })),
          })
        : "",
    [raisedClasses, raised],
  );
  const flowing = useMemo(() => {
    const s = new Set<string>();
    for (const [model, names] of Object.entries(allow)) {
      const m = d.models.get(model)?.model;
      for (const n of names) {
        const f = m?.fields.find((x) => x.name === n);
        if (!f) continue;
        const denied =
          raisedClasses.includes(f.class) && max.includes(f.class)
            ? false
            : defaultTreatment(f, max) === "deny";
        if (!denied) s.add(f.class);
      }
    }
    return [...s];
  }, [allow, d.models, raisedClasses, max]);

  const missing = cfg.filter(
    (f) =>
      f.required &&
      !(
        f.secret ? (f.key === "signing_secret" ? secret : values[`s:${f.key}`]) : values[f.key]
      )?.trim(),
  );
  const chosenFields = Object.values(allow).reduce((n, l) => n + l.length, 0);
  const idx = steps.indexOf(step);
  const canNext =
    step === "provider"
      ? !!provider
      : step === "settings"
        ? !!name.trim() && missing.length === 0
        : step === "fields"
          ? chosenFields > 0 && (!inbound || Object.keys(allow).includes(target))
          : step === "basis"
            ? purpose.trim().length >= 10 &&
              confirmed &&
              (basis !== "legitimate_interest" || note.trim().length >= 20)
            : true;

  const secretOf = (key: string) =>
    key === "signing_secret" ? secret : (values[`s:${key}`] ?? "");

  async function save() {
    if (!provider) return;
    setBusy(true);
    setError(null);
    let id = connection;
    if (!id) {
      const config: Record<string, string> = {};
      const credentials: Record<string, string> = {};
      for (const f of cfg) {
        if (f.secret) {
          if (secretOf(f.key).trim()) credentials[f.key] = secretOf(f.key).trim();
        } else if ((values[f.key] ?? "").trim()) config[f.key] = values[f.key]!.trim();
      }
      const r = await d.run("connection.create", {
        name: name.trim(),
        provider: provider.id,
        config: JSON.stringify(config),
        credentials: JSON.stringify(credentials),
        field_allowlist: JSON.stringify(allow),
        policy,
      });
      if (r.kind !== "ok") {
        setBusy(false);
        return setError(
          r.kind === "approval"
            ? t("Approval needed: the connection was proposed as plan {plan}.", {
                plan: r.planId.slice(0, 8),
              })
            : serverMessage(r.message),
        );
      }
      id = (r.value as { id: string }).id;
      setConnection(id);
    }
    if (!consentDone) {
      const r = await d.run("consent.create", {
        connection: id,
        purpose: purpose.trim(),
        legal_basis: basis,
        note: note.trim(),
      });
      if (r.kind !== "ok") {
        setBusy(false);
        return setError(r.kind === "approval" ? t("Approval needed.") : serverMessage(r.message));
      }
      setConsentDone(true);
    }
    setBusy(false);
    setStep("connect");
  }

  async function test() {
    if (!connection) return;
    setBusy(true);
    setError(null);
    setChecks(null);
    const r = await d.run("connection.test", { id: connection });
    setBusy(false);
    if (r.kind !== "ok")
      return setError(r.kind === "error" ? serverMessage(r.message) : t("Needs approval"));
    const res = parseJson<{ checks?: string[] }>((r.value as { result: string }).result, {});
    setChecks(res.checks ?? []);
  }

  async function connect() {
    if (!connection) return;
    setBusy(true);
    setError(null);
    const r = await d.run("connection.connect", { id: connection });
    setBusy(false);
    if (r.kind === "ok") {
      d.notify(t("Connected"));
      return props.onDone(connection);
    }
    if (r.kind === "approval") {
      d.notify(
        t(
          "Approval needed: connecting was proposed as plan {plan}. Nothing changes until a human approves it.",
          {
            plan: r.planId.slice(0, 8),
          },
        ),
        "info",
      );
      return props.onDone(connection);
    }
    setError(serverMessage(r.message));
  }

  async function authorise() {
    if (!connection) return;
    const r = await d.run("connection.oauth_start", { id: connection });
    if (r.kind !== "ok")
      return setError(r.kind === "error" ? serverMessage(r.message) : t("Needs approval"));
    const url = parseJson<{ url?: string }>((r.value as { result: string }).result, {}).url;
    if (url) window.open(url, "_blank", "noopener,noreferrer");
  }

  const next = () => {
    setError(null);
    if (step === "basis") return void save();
    setStep(steps[idx + 1]!);
  };
  const back = () => {
    setError(null);
    setStep(steps[idx - 1]!);
  };
  const webhookUrl =
    connection && provider?.engine === "webhook_in"
      ? `${location.origin}/hooks/in/${connection}`
      : "";
  const choices = props.providers.filter((p) => p.maturity !== "definition_only");

  return (
    <Dialog
      title={t("Connect a service")}
      size="lg"
      onClose={() => props.onDone(connection ?? undefined)}
      footer={
        <>
          {step !== "provider" && step !== "connect" && !(idx === 1 && startProvider) && (
            <Button onClick={back}>{t("Back")}</Button>
          )}
          {step === "connect" ? (
            <>
              <Button onClick={() => props.onDone(connection ?? undefined)}>
                {t("Finish later")}
              </Button>
              <Button variant="primary" loading={busy} onClick={() => void connect()}>
                {t("Connect")}
              </Button>
            </>
          ) : (
            <Button variant="primary" disabled={!canNext} loading={busy} onClick={next}>
              {step === "basis" ? t("Save and continue") : t("Next")}
            </Button>
          )}
        </>
      }
    >
      <ol className="cx-steps" aria-label={t("Steps")}>
        {steps.map((s, i) => (
          <li
            key={s}
            aria-current={s === step ? "step" : undefined}
            data-done={i < idx || undefined}
          >
            <span className="cx-step-n" aria-hidden="true">
              {i + 1}
            </span>
            {stepLabel(s)}
          </li>
        ))}
      </ol>
      <p className="hint cx-disclaimer">
        {t(
          "These controls support GDPR compliance. They are not legal advice or a certification: the purposes, the legal bases and the agreements remain yours to decide and to document.",
        )}
      </p>
      {error && (
        <Notice tone="danger" title={t("This did not work")}>
          {error}
        </Notice>
      )}

      {step === "provider" && (
        <div role="radiogroup" aria-label={t("Service")} className="cx-choices">
          {choices.length === 0 && (
            <Notice tone="info" title={t("No service is enabled yet")}>
              {t(
                "An administrator enables a service in the catalogue and records the agreement with the provider.",
              )}
            </Notice>
          )}
          {choices.map((p) => (
            <label key={p.id} className="cx-choice" data-on={provider?.id === p.id || undefined}>
              <input
                type="radio"
                name="provider"
                checked={provider?.id === p.id}
                disabled={!p.enabled}
                onChange={() => choose(p)}
              />
              <span>
                <strong>{p.name}</strong>{" "}
                <Badge tone={maturityTone(p.maturity)}>{maturityLabel(p.maturity)}</Badge>{" "}
                <Badge tone="neutral" dot={false}>
                  {directionLabel(p.direction)}
                </Badge>
                <span className="hint block">{p.purpose}</span>
                {!p.enabled && (
                  <span className="hint block">
                    {t("Not enabled yet: an administrator enables it in the catalogue.")}
                  </span>
                )}
              </span>
            </label>
          ))}
        </div>
      )}

      {step === "settings" && provider && (
        <div className="wform">
          <Input
            label={t("Name")}
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            hint={t("A name that tells you what this connection is for.")}
          />
          {cfg.map((f) => {
            if (f.key === "target_model")
              return (
                <Select
                  key={f.key}
                  label={f.label}
                  required={f.required}
                  hint={f.help}
                  value={values.target_model ?? ""}
                  onChange={(e) => {
                    setValues({ ...values, target_model: e.target.value });
                    setAllow({});
                  }}
                >
                  <option value="">{t("Choose a model")}</option>
                  {models
                    .filter((m) => m.model.can_create && !blockedModel(m.model.name))
                    .map((m) => (
                      <option key={m.model.name} value={m.model.name}>
                        {modelTitle(m.model, m.module.id)}
                      </option>
                    ))}
                </Select>
              );
            if (f.secret)
              return (
                <div key={f.key} className="cx-secret">
                  <Input
                    label={f.label}
                    required={f.required}
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    value={secretOf(f.key)}
                    hint={`${f.help} ${t("Write-only: it is stored encrypted and never shown again.")}`}
                    onChange={(e) =>
                      f.key === "signing_secret"
                        ? setSecret(e.target.value)
                        : setValues({ ...values, [`s:${f.key}`]: e.target.value })
                    }
                  />
                  {f.key === "signing_secret" && (
                    <Button size="sm" onClick={() => setSecret(newSecret())}>
                      {t("Generate a secret")}
                    </Button>
                  )}
                </div>
              );
            return f.key === "mapping" ? (
              <Textarea
                key={f.key}
                label={f.label}
                rows={3}
                spellCheck={false}
                value={values[f.key] ?? ""}
                hint={f.help}
                onChange={(e) => setValues({ ...values, [f.key]: e.target.value })}
              />
            ) : (
              <Input
                key={f.key}
                label={f.label}
                required={f.required}
                value={values[f.key] ?? ""}
                hint={f.help}
                onChange={(e) => setValues({ ...values, [f.key]: e.target.value })}
              />
            );
          })}
          {secret && provider.engine !== "webhook_in" && provider.engine !== "none" && (
            <p className="hint">
              {t("Give the same secret to the receiver so it can verify each delivery.")}
            </p>
          )}
          {secret.startsWith("whsec_") && (
            <Notice tone="warning" title={t("Copy the secret now")}>
              {secret}
            </Notice>
          )}
          {provider.setup && (
            <details>
              <summary>{t("Setup instructions")}</summary>
              <p>{provider.setup}</p>
            </details>
          )}
        </div>
      )}

      {step === "fields" && provider && (
        <>
          <p>
            {inbound
              ? t(
                  "Choose the fields this connection may write. A field that is not chosen is refused, not dropped.",
                )
              : t(
                  "Choose the fields that may leave. Only these are sent, and each is treated by its data class.",
                )}
          </p>
          <FieldPicker
            models={models}
            providerMax={max}
            inbound={!!inbound}
            value={allow}
            onChange={setAllow}
            raised={raisedClasses}
            only={inbound ? target : undefined}
          />
          {highRisk.length > 0 && (
            <fieldset className="cx-ack">
              <legend>{t("High-risk data")}</legend>
              {highRisk.map((c) => (
                <div key={c}>
                  {!max.includes(c) ? (
                    <Notice tone="info" title={className(c)}>
                      {t(
                        "This service cannot receive this class of data, so these fields are never sent.",
                      )}
                    </Notice>
                  ) : (
                    <>
                      <Checkbox
                        label={t("Let {class} data go, as pseudonyms", { class: className(c) })}
                        hint={t(
                          "It stays withheld unless you record an impact assessment. Your name and the time are recorded with it.",
                        )}
                        checked={c in raised}
                        onChange={(e) => {
                          const next = { ...raised };
                          if (e.target.checked) next[c] = "";
                          else delete next[c];
                          setRaised(next);
                        }}
                      />
                      {c in raised && (
                        <Textarea
                          label={t("Impact assessment for {class}", { class: className(c) })}
                          rows={2}
                          value={raised[c]}
                          hint={t(
                            "Who decided, why it is necessary, how long it is kept (at least 30 characters).",
                          )}
                          onChange={(e) => setRaised({ ...raised, [c]: e.target.value })}
                        />
                      )}
                    </>
                  )}
                </div>
              ))}
            </fieldset>
          )}
          {chosenFields > 0 && (
            <>
              {Object.keys(allow).length > 1 && (
                <Select
                  label={t("Preview this model")}
                  value={previewModel || Object.keys(allow)[0]}
                  onChange={(e) => setPreviewModel(e.target.value)}
                >
                  {Object.keys(allow).map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </Select>
              )}
              <PrivacyPreview
                provider={provider.id}
                model={
                  Object.keys(allow).includes(previewModel)
                    ? previewModel
                    : (Object.keys(allow).find((m) => allow[m]!.length) ?? "")
                }
                allow={allow}
                policy={policy}
                inbound={!!inbound}
              />
            </>
          )}
        </>
      )}

      {step === "basis" && provider && (
        <div className="wform">
          <section className="cx-recipient" aria-label={t("Who receives the data")}>
            <h3>{t("Who receives the data")}</h3>
            <dl className="cx-facts">
              <dt>{t("Recipient")}</dt>
              <dd>{provider.legal_name}</dd>
              <dt>{t("Country")}</dt>
              <dd>{provider.country}</dd>
              <dt>{t("Transfers")}</dt>
              <dd>{adequacyLabel(provider.eu_adequate)}</dd>
              <dt>{t("Agreement")}</dt>
              <dd>
                {provider.dpa_recorded ? t("Recorded by an administrator") : t("Not recorded")}
              </dd>
            </dl>
            <p className="hint">{provider.verify_note}</p>
          </section>
          <section aria-label={t("Data classes that would flow")}>
            <h3>{t("Data classes that would flow")}</h3>
            <p>
              {flowing.length
                ? flowing.map((c) => className(c)).join(", ")
                : t("None of the chosen fields is allowed to flow.")}
            </p>
          </section>
          {provider.legal_basis_text && <Notice tone="info">{provider.legal_basis_text}</Notice>}
          <Textarea
            label={t("Purpose")}
            required
            rows={2}
            value={purpose}
            hint={t("What is the data used for? At least 10 characters.")}
            onChange={(e) => setPurpose(e.target.value)}
          />
          <Select
            label={t("Legal basis")}
            required
            value={basis}
            disabled={!props.isAdmin && !!provider.legal_basis}
            onChange={(e) => setBasis(e.target.value)}
          >
            {BASES.map((b) => (
              <option key={b} value={b}>
                {basisLabel(b)}
              </option>
            ))}
          </Select>
          {basis === "legitimate_interest" && (
            <Textarea
              label={t("Balancing test")}
              required
              rows={3}
              value={note}
              hint={t(
                "Why your interest is not outweighed by the interests of the people concerned (at least 20 characters).",
              )}
              onChange={(e) => setNote(e.target.value)}
            />
          )}
          <Checkbox
            label={t(
              "I confirm the purpose and the legal basis, and I know the data classes listed above would flow.",
            )}
            hint={t(
              "You can withdraw this at any time. Withdrawing disconnects the service and removes the stored credentials.",
            )}
            checked={confirmed}
            onChange={(e) => setConfirmed(e.target.checked)}
          />
        </div>
      )}

      {step === "connect" && provider && (
        <div className="wform">
          <Notice tone="success" title={t("Saved as a draft with its legal basis")}>
            {t("Nothing flows until you connect.")}
          </Notice>
          {webhookUrl && (
            <section>
              <h3>{t("Where the sender posts")}</h3>
              <p className="hint">
                {t(
                  "Configure the sender to POST here and to sign each request with the secret you chose.",
                )}
              </p>
              <pre className="cx-url">{webhookUrl}</pre>
            </section>
          )}
          {provider.auth_kind === "oauth2" && (
            <Button onClick={() => void authorise()}>{t("Authorise with the provider")}</Button>
          )}
          <div className="actionrow">
            <Button loading={busy} onClick={() => void test()}>
              {t("Test connection")}
            </Button>
          </div>
          {checks && (
            <Notice tone="success" title={t("The test passed")}>
              {checks.join(". ")}
            </Notice>
          )}
        </div>
      )}
    </Dialog>
  );
}
