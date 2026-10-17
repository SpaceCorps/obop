import { useMemo } from "react";
import { Badge, t } from "@obop/ui";
import type { FieldMeta } from "../api.ts";
import type { ModelInfo } from "../data.tsx";
import { fieldTitle, modelTitle } from "../labels.ts";
import { Button, Checkbox, Select } from "../ui.tsx";
import {
  className,
  defaultTreatment,
  blockedModel,
  listable,
  treatmentLabel,
  type Treatment,
} from "./model.ts";

type Props = {
  models: ModelInfo[];
  /** Data classes the provider may receive at most. */
  providerMax: string[];
  inbound: boolean;
  value: Record<string, string[]>;
  onChange: (v: Record<string, string[]>) => void;
  /** Classes the owner raised and acknowledged: shown as sent as a pseudonym instead of never. */
  raised: string[];
  /** Only these models can be chosen (an inbound connection has one target). */
  only?: string;
};

const tone = (tr: Treatment) =>
  tr === "deny" ? "danger" : tr === "pseudonymise" ? "info" : ("success" as const);

/**
 * Pick the fields that may leave (or be accepted). Each one shows its data class and what the
 * policy does with it, so nothing is sent by surprise: an empty choice sends nothing.
 */
export function FieldPicker(p: Props) {
  const chosen = Object.keys(p.value);
  const available = useMemo(
    () =>
      p.models
        .filter((x) => !blockedModel(x.model.name))
        .filter((x) => !p.only || x.model.name === p.only)
        .filter((x) => x.model.fields.some(listable)),
    [p.models, p.only],
  );
  const addable = available.filter((x) => !chosen.includes(x.model.name));
  const toggle = (model: string, field: string, on: boolean) => {
    const cur = p.value[model] ?? [];
    p.onChange({
      ...p.value,
      [model]: on ? [...cur, field] : cur.filter((f) => f !== field),
    });
  };
  const treat = (f: FieldMeta): Treatment =>
    p.raised.includes(f.class) && p.providerMax.includes(f.class)
      ? "pseudonymise"
      : defaultTreatment(f, p.providerMax);
  return (
    <div className="cx-picker">
      {chosen.length === 0 && (
        <p className="hint">
          {t("Nothing is selected, so nothing would be sent. Choose a model, then its fields.")}
        </p>
      )}
      {chosen.map((name) => {
        const info = available.find((x) => x.model.name === name);
        if (!info) return null;
        const fields = info.model.fields.filter(listable);
        return (
          <fieldset key={name} className="cx-model">
            <legend>{modelTitle(info.model, info.module.id)}</legend>
            <ul className="cx-fields">
              {fields.map((f) => {
                const tr = treat(f);
                const on = (p.value[name] ?? []).includes(f.name);
                return (
                  <li key={f.name}>
                    <Checkbox
                      label={fieldTitle(f)}
                      checked={on}
                      onChange={(e) => toggle(name, f.name, e.target.checked)}
                    />
                    <span className="cx-field-meta">
                      <Badge tone="neutral" dot={false}>
                        {className(f.class)}
                      </Badge>
                      {f.personal_data && (
                        <Badge tone="warning" dot={false}>
                          {t("Personal data")}
                        </Badge>
                      )}
                      <Badge tone={tone(tr)}>{treatmentLabel(tr, p.inbound)}</Badge>
                    </span>
                  </li>
                );
              })}
            </ul>
            {!p.only && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  const next = { ...p.value };
                  delete next[name];
                  p.onChange(next);
                }}
              >
                {t("Remove this model")}
              </Button>
            )}
          </fieldset>
        );
      })}
      {addable.length > 0 && !(p.only && chosen.length > 0) && (
        <Select
          label={t("Add a model")}
          value=""
          onChange={(e) => e.target.value && p.onChange({ ...p.value, [e.target.value]: [] })}
        >
          <option value="">{t("Choose a model")}</option>
          {addable.map((x) => (
            <option key={x.model.name} value={x.model.name}>
              {modelTitle(x.model, x.module.id)}
            </option>
          ))}
        </Select>
      )}
    </div>
  );
}
