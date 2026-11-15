// "Set up your workspace": the first steps of a new instance, on the home screen of an
// administrator until they are done or hidden. Each step is checked against the instance itself
// (the brand, the settings, the people and the contacts there are), so it ticks itself off however
// the step was done, here, from the command line or by an agent. Hiding the card and the step
// "choose apps" are kept in the instance settings (`setting.update`), the same for every admin.
import { useEffect, useState } from "react";
import { Button, Icon, pageBrand, t } from "@obop/ui";
import { aggregate, call, loadSettings, saveSettings, type Settings } from "./api.ts";
import { modulesHref } from "./modules/route.ts";
import { href } from "./route.ts";

/** What the settings keep of the setup (the settings document accepts any key). */
type Setup = { dismissed?: boolean; done?: string[] };
type WithSetup = Settings & { setup?: Setup };

type Step = { id: string; title: string; hint: string; to: string; done: boolean };

/** How many accounts the instance has (`user_account` has `list` only: no records of its own). */
async function peopleCount(token: string): Promise<number | null> {
  const r = await call(token, "user_account.list", {});
  return r.kind === "ok" && Array.isArray(r.value) ? r.value.length : null;
}

/** How many records a model has, or null when it cannot be counted (no access, no such model). */
async function countOf(token: string, model: string): Promise<number | null> {
  const r = await aggregate(token, model, { measures: [{ op: "count" }] });
  return "error" in r ? null : r.total;
}

export function SetupCard({
  token,
  settings,
  onSettings,
  hasContacts,
}: {
  token: string;
  settings: Settings;
  onSettings: (s: Settings) => void;
  /** Whether the Contacts app is installed and readable. */
  hasContacts: boolean;
}) {
  const setup = (settings as WithSetup).setup ?? {};
  const [counts, setCounts] = useState<{ people: number | null; contacts: number | null } | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (setup.dismissed) return;
    let live = true;
    void Promise.all([
      peopleCount(token),
      hasContacts ? countOf(token, "party") : Promise.resolve(null),
    ]).then(([people, contacts]) => live && setCounts({ people, contacts }));
    return () => {
      live = false;
    };
  }, [token, hasContacts, setup.dismissed]);

  if (setup.dismissed || !counts) return null;

  const steps: Step[] = [
    {
      id: "identity",
      title: t("Name and logo"),
      hint: t("How your workspace is called, on every page and on the sign-in screen."),
      to: href({ app: "customize", model: "identity", tab: "identity" }),
      done: !!pageBrand(),
    },
    {
      id: "region",
      title: t("Language and currency"),
      hint: t("The language everyone starts with and the currency of amounts."),
      to: href({ app: "customize", model: "theme", tab: "theme" }),
      done: !!settings.language && !!settings.currency,
    },
    {
      id: "people",
      title: t("Invite people"),
      hint: t("Give your colleagues their own sign-in and the apps they need."),
      to: href({ app: "customize", model: "user_account" }),
      done: (counts.people ?? 0) > 1,
    },
    ...(hasContacts
      ? [
          {
            id: "contacts",
            title: t("Import your contacts"),
            hint: t("Bring customers and suppliers in from a CSV file."),
            to: href({ app: "base", model: "party" }),
            done: (counts.contacts ?? 0) > 0,
          },
        ]
      : []),
    {
      id: "apps",
      title: t("Choose your apps"),
      hint: t("Install what you need from the modules; everything stays free."),
      to: modulesHref({}),
      done: !!setup.done?.includes("apps") || !!settings.apps,
    },
  ];
  const done = steps.filter((s) => s.done).length;
  if (done === steps.length) return null;

  /** Keep a change of the setup in the instance settings, on top of what is stored now. */
  const keep = async (patch: Setup) => {
    setBusy(true);
    const fresh = (await loadSettings(token)) as WithSetup;
    const next: WithSetup = { ...fresh, setup: { ...fresh.setup, ...patch } };
    const r = await saveSettings(token, next);
    setBusy(false);
    if (r.kind === "ok") onSettings(next);
  };

  return (
    <section className="setup-card" aria-labelledby="setup-h">
      <header className="setup-head">
        <div>
          <h2 id="setup-h">{t("Set up your workspace")}</h2>
          <p className="hint">
            {t("{done} of {all} done", { done: String(done), all: String(steps.length) })}
          </p>
        </div>
        <Button variant="ghost" disabled={busy} onClick={() => void keep({ dismissed: true })}>
          {t("Hide")}
        </Button>
      </header>
      <div
        className="setup-bar"
        role="progressbar"
        aria-label={t("Set up your workspace")}
        aria-valuemin={0}
        aria-valuemax={steps.length}
        aria-valuenow={done}
      >
        <span style={{ inlineSize: `${(done / steps.length) * 100}%` }} />
      </div>
      <ol className="setup-steps">
        {steps.map((s, i) => {
          const body = (
            <>
              <span className="setup-mark" aria-hidden="true">
                {s.done ? <Icon name="check" size={14} /> : i + 1}
              </span>
              <span className="setup-text">
                <strong>{s.title}</strong>
                <small>{s.hint}</small>
              </span>
            </>
          );
          return (
            <li key={s.id} className={s.done ? "is-done" : undefined}>
              {s.done ? (
                <div className="setup-step done">
                  {body}
                  <span className="sr-only">{t("Done")}</span>
                </div>
              ) : (
                <a
                  className="setup-step"
                  href={s.to}
                  onClick={() => {
                    if (s.id === "apps")
                      void keep({ done: [...new Set([...(setup.done ?? []), "apps"])] });
                  }}
                >
                  {body}
                  <Icon name="chevron-right" size={16} />
                </a>
              )}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
