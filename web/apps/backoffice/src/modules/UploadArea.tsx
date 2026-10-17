import { useEffect, useId, useState } from "react";
import { Button, Input, Notice, Textarea, t } from "@obop/ui";
import { serverMessage } from "../serverMessage.ts";
import { preview } from "./data.ts";
import { BlockerList, PlanSteps } from "./PlanDialog.tsx";
import type { Plan } from "./types.ts";

type Check =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "plan"; plan: Plan }
  | { state: "error"; message: string };

/**
 * "Upload your own package": choose an obop.toml (or paste its text) and see at once, from
 * `module_plan.preview`, whether it parses and what installing it would do: the dependencies it
 * brings, the core it needs, and what blocks it. Nothing is installed from here; Install opens the
 * same plan dialog as the marketplace.
 */
export function UploadArea({
  token,
  nameOf,
  onInstall,
  onBuild,
}: {
  token: string;
  nameOf: (id: string) => string;
  onInstall: (manifest: string) => void;
  onBuild: () => void;
}) {
  const [text, setText] = useState("");
  const [fileName, setFileName] = useState("");
  const [pasting, setPasting] = useState(false);
  const [check, setCheck] = useState<Check>({ state: "idle" });
  const id = useId();

  // Check as the text changes, a little after the last keystroke.
  useEffect(() => {
    if (!text.trim()) {
      setCheck({ state: "idle" });
      return;
    }
    let live = true;
    setCheck({ state: "checking" });
    const timer = setTimeout(() => {
      void preview(token, { manifest: text }).then((r) => {
        if (!live) return;
        setCheck(
          "plan" in r
            ? { state: "plan", plan: r.plan }
            : { state: "error", message: serverMessage(r.error) },
        );
      });
    }, 350);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [text, token]);

  const plan = check.state === "plan" ? check.plan : null;
  const work = plan?.steps.filter((s) => s.action !== "skip").length ?? 0;
  const nm = (m: string) => nameOf(m) || m;
  return (
    <section className="stack mods-upload" aria-labelledby={`${id}-h`}>
      <div className="mods-upload-head">
        <div>
          <h3 id={`${id}-h`} className="overline">
            {t("Upload your own package")}
          </h3>
          <p className="hint">
            {t(
              "An obop.toml file: models, fields with data classes, permission groups and actions. It is checked here before anything is installed.",
            )}
          </p>
        </div>
        <Button variant="ghost" size="sm" icon="edit" onClick={onBuild}>
          {t("Build a module instead")}
        </Button>
      </div>
      <div className="mods-upload-inputs">
        <Input
          type="file"
          accept=".toml"
          aria-label={t("Upload an obop.toml file")}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            setFileName(file.name);
            setText(await file.text());
            setPasting(false);
          }}
        />
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={pasting}
          onClick={() => setPasting(!pasting)}
        >
          {pasting ? t("Hide the text") : t("Paste the text instead")}
        </Button>
      </div>
      {pasting && (
        <Textarea
          label={t("Text of the obop.toml")}
          rows={8}
          spellCheck={false}
          className="code-input"
          value={text}
          onChange={(e) => {
            setFileName("");
            setText(e.target.value);
          }}
        />
      )}
      <div aria-live="polite" className="stack mods-check">
        {check.state === "checking" && <p className="hint">{t("Checking the package…")}</p>}
        {check.state === "error" && (
          <Notice tone="danger" title={t("This package cannot be read")}>
            {check.message}
          </Notice>
        )}
        {plan && (
          <>
            <Notice
              tone={plan.ok ? "success" : "danger"}
              title={
                plan.ok
                  ? fileName
                    ? t("{file} is a valid package", { file: fileName })
                    : t("This is a valid package")
                  : t("This package cannot be installed here")
              }
            >
              {plan.ok
                ? t("Core {version} is running. Nothing is installed until you confirm.", {
                    version: plan.core_version,
                  })
                : t("Core {version} is running.", { version: plan.core_version })}
            </Notice>
            {plan.blockers.length > 0 && <BlockerList blockers={plan.blockers} nameOf={nm} />}
            {plan.steps.length > 0 && <PlanSteps plan={plan} nameOf={nm} />}
            {plan.warnings.length > 0 && (
              <Notice tone="warning" title={t("Worth knowing")}>
                {plan.warnings.join(" ")}
              </Notice>
            )}
            <div>
              <Button
                variant="primary"
                disabled={!plan.ok || work === 0}
                onClick={() => onInstall(text)}
              >
                {t("Install")}
              </Button>
            </div>
          </>
        )}
      </div>
    </section>
  );
}
