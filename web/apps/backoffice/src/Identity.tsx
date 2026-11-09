import { useEffect, useId, useRef, useState } from "react";
import {
  Button,
  InstanceLogo,
  Input,
  LANGS,
  LANG_INFO,
  Notice,
  Select,
  Skeleton,
  Switch,
  brandLogoUrl,
  brandMessageIn,
  getLang,
  t,
} from "@obop/ui";
import { call } from "./api.ts";
import { useData } from "./data.tsx";
import {
  LIMITS,
  artworkProblem,
  blankForm,
  changes,
  clip,
  loadBrand,
  previewBrand,
  type Form,
  type Lang,
  type Loaded,
  type LogoChange,
} from "./identity.ts";
import { setProductDocsHidden } from "./productDocs.ts";

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/** The seven languages by their own names, which are never translated. */
const NAMES = LANGS.map((lang) => ({ lang, native: LANG_INFO[lang].native }));

/** An address that shows nothing: the logo of a preview before anything is saved. */
const BLANK = "data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg'/%3E";

/** The language the preview starts in: the one shown, when it is one of the seven. */
function startLang(): Lang {
  const shown = getLang();
  return (LANGS as readonly string[]).includes(shown) ? (shown as Lang) : "en";
}

/**
 * Customize, Identity (administrators). The name, the logo and the texts that an instance shows
 * instead of the product's own, on the sign-in screens, in the tab, in the top bar and on the pages
 * of connected apps. A form over `instance_brand.get`, `configure` and `clear`
 * and nothing else, with a preview of the sign-in card and of the tile.
 */
export function IdentityTab({ token, notify }: { token: string; notify: Notify }) {
  const { confirm } = useData();
  const ids = useId();
  const [loaded, setLoaded] = useState<Loaded | null | undefined>(undefined);
  const [form, setForm] = useState<Form>(blankForm);
  const [logo, setLogo] = useState<LogoChange>({ kind: "keep" });
  const [pending, setPending] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [fileError, setFileError] = useState("");
  const [previewLang, setPreviewLang] = useState<Lang>(startLang);
  // Counts the saves that went through; the effect below reads it to put the focus back.
  const [saves, setSaves] = useState(0);
  const file = useRef<HTMLInputElement>(null);
  const nameId = `${ids}-name`;

  // `sent` is the form and the logo choice a save has just sent. A person who went on typing while
  // the save ran keeps what was typed: the form is replaced by the record only when it is still
  // what was sent.
  async function load(sent?: { form: Form; logo: LogoChange }) {
    const next = await loadBrand(token);
    setLoaded(next);
    if (next) {
      setForm((now) => (sent === undefined || now === sent.form ? next.form : now));
      setLogo((now) => (sent === undefined || now === sent.logo ? { kind: "keep" } : now));
    }
  }
  useEffect(() => {
    void load();
  }, [token]);

  // Save turns itself off when the form is read again, and a button that is off cannot keep the
  // focus: a person who saved with the keyboard would be left on nothing, at the end of the page.
  // The focus goes to the first field, unless it is somewhere that works.
  useEffect(() => {
    if (saves === 0) return;
    const at = document.activeElement as HTMLElement | null;
    if (!at || at === document.body || (at as HTMLButtonElement).disabled)
      document.getElementById(nameId)?.focus();
  }, [saves, nameId]);

  // A chosen file is previewed from a local address that is let go when it changes or the tab closes.
  useEffect(() => {
    if (logo.kind !== "replace") return setPending("");
    const url = URL.createObjectURL(new Blob([logo.text], { type: "image/svg+xml" }));
    setPending(url);
    return () => URL.revokeObjectURL(url);
  }, [logo]);

  if (loaded === undefined) return <Skeleton rows={6} cols={2} />;
  if (loaded === null)
    return (
      <Notice tone="danger" title={t("The identity of this instance could not be read")}>
        {t("The server did not answer, or answered with an error. Try again in a moment.")}
      </Notice>
    );

  const input = changes(
    loaded.form,
    form,
    logo,
    loaded.fallbacks.map((f) => f.field),
  );
  const dirty = Object.keys(input).length > 0;
  const needsName = !loaded.set && !form.name.trim();
  const preview = previewBrand(form, loaded.v);
  const hasLogo = logo.kind === "replace" || (logo.kind === "keep" && loaded.logo === "artwork");
  const removing = logo.kind === "remove";
  // The card draws the saved logo, a file that was chosen, or nothing: before the first save, and
  // when the artwork is to be removed (the tile that takes its place is drawn by the server once
  // the removal is saved).
  const cardSrc = logo.kind === "replace" ? pending : removing || !loaded.set ? BLANK : undefined;
  const cardMessage = brandMessageIn(preview, previewLang);

  // The texts are cut at the limit in characters as the server counts them (code points). The
  // `maxLength` of an input counts UTF-16 units and would cut a letter outside the Basic
  // Multilingual Plane at half the length; the server stays the judge of anything else.
  const setText = (patch: Partial<Form>) => setForm((f) => ({ ...f, ...patch }));
  const setTitle = (l: Lang, v: string) =>
    setForm((f) => ({ ...f, titles: { ...f.titles, [l]: clip(v, LIMITS.title) } }));
  const setMessage = (l: Lang, v: string) =>
    setForm((f) => ({ ...f, messages: { ...f.messages, [l]: clip(v, LIMITS.message) } }));

  async function chosen(f: File | undefined) {
    setFileError("");
    if (!f) return;
    const problem = artworkProblem(f);
    if (problem === "big")
      return setFileError(t("The file is larger than 16 KiB. Choose a smaller SVG file."));
    if (problem === "kind") return setFileError(t("Choose an SVG file."));
    setLogo({ kind: "replace", text: await f.text(), file: f.name });
  }

  async function save() {
    setBusy(true);
    setError("");
    const sent = { form, logo };
    const r = await call(token, "instance_brand.configure", input);
    setBusy(false);
    if (r.kind !== "ok") {
      setError(r.kind === "error" ? r.message : t("The server asked for an approval."));
      return;
    }
    setProductDocsHidden(form.hideDocs);
    notify(
      t(
        "Identity saved. The tab title and the logo in the top bar change when the page loads again.",
      ),
    );
    await load(sent);
    setSaves((n) => n + 1);
  }

  async function remove() {
    const ok = await confirm(
      t(
        "Remove the identity of this instance? Every screen shows the product's own name and mark again, and the Brand book and the Guide are shown.",
      ),
      t("Remove"),
      t("Keep"),
    );
    if (!ok) return;
    setBusy(true);
    setError("");
    const r = await call(token, "instance_brand.clear");
    setBusy(false);
    if (r.kind !== "ok") {
      setError(r.kind === "error" ? r.message : t("The server asked for an approval."));
      return;
    }
    setProductDocsHidden(false);
    notify(
      t(
        "Identity removed. The tab title and the logo in the top bar change when the page loads again.",
      ),
    );
    await load();
  }

  const slugNow = form.slug || loaded.slug;

  return (
    <form
      className="two identity-tab"
      noValidate
      onSubmit={(e) => {
        // Enter in a field saves, as the Save button does, and only when Save is on.
        e.preventDefault();
        if (dirty && !needsName && !busy) void save();
      }}
    >
      <div className="stack gap-lg">
        <Notice tone="info" title={t("Shown before anyone signs in")}>
          {t(
            "The name, the logo, the messages and the titles can be read by anyone who opens the sign-in page, with no account. Do not put anything private in them.",
          )}
        </Notice>
        {loaded.fallbacks.length > 0 && (
          <Notice tone="warning" title={t("Some stored values cannot be used")}>
            {t("The default is shown instead of: {fields}. Type a new value to replace them.", {
              fields: loaded.fallbacks.map((f) => f.field).join(", "),
            })}
          </Notice>
        )}
        <div className="stack">
          <Input
            id={nameId}
            label={t("Name")}
            hint={t("Shown in the tab, on the sign-in screens and in the top bar.")}
            value={form.name}
            required={!loaded.set}
            autoComplete="off"
            onChange={(e) => setText({ name: clip(e.target.value, LIMITS.name) })}
          />
          <Input
            label={t("Short name")}
            hint={t(
              "Lower-case letters, digits and hyphens. Names the files people download, such as the recovery codes. Leave it empty to take it from the name; now: {slug}.",
              { slug: slugNow || "…" },
            )}
            value={form.slug}
            autoComplete="off"
            onChange={(e) => setText({ slug: clip(e.target.value, LIMITS.slug) })}
          />
        </div>
        <div className="stack">
          <h3 className="overline">{t("Logo")}</h3>
          <p className="hint">
            {t(
              "An SVG file of at most 16 KiB. Without one, a tile with the first letter of the name is drawn.",
            )}
          </p>
          <div className="actionrow">
            <input
              ref={file}
              type="file"
              accept=".svg,image/svg+xml"
              className="sr-only"
              tabIndex={-1}
              aria-hidden="true"
              aria-label={t("SVG file")}
              onChange={(e) => {
                void chosen(e.target.files?.[0]);
                e.target.value = "";
              }}
            />
            <Button icon="upload" onClick={() => file.current?.click()}>
              {t("Choose a file")}
            </Button>
            {hasLogo && (
              <Button variant="ghost" icon="trash" onClick={() => setLogo({ kind: "remove" })}>
                {t("Remove the logo")}
              </Button>
            )}
            {logo.kind !== "keep" && (
              <Button variant="ghost" onClick={() => setLogo({ kind: "keep" })}>
                {t("Undo")}
              </Button>
            )}
          </div>
          {logo.kind === "replace" && (
            <p className="hint" role="status">
              {t("{file} will replace the logo when you save.", { file: logo.file })}
            </p>
          )}
          {logo.kind === "remove" && (
            <p className="hint" role="status">
              {t("The logo will be removed when you save.")}
            </p>
          )}
          {fileError && (
            <p className="hint identity-error" role="alert">
              {fileError}
            </p>
          )}
        </div>
        <div className="stack" role="group" aria-labelledby={`${ids}-msg`}>
          <h3 id={`${ids}-msg`} className="overline">
            {t("Message on the sign-in screen")}
          </h3>
          <p className="hint">
            {t(
              "Shown under the sign-in line in the language of the screen. A language without one shows the English message.",
            )}
          </p>
          {NAMES.map(({ lang, native }) => (
            <Input
              key={lang}
              label={native}
              lang={lang}
              value={form.messages[lang]}
              autoComplete="off"
              onChange={(e) => setMessage(lang, e.target.value)}
            />
          ))}
        </div>
        <div className="stack" role="group" aria-labelledby={`${ids}-title`}>
          <h3 id={`${ids}-title`} className="overline">
            {t("Titles on the pages of connected apps")}
          </h3>
          <p className="hint">
            {t(
              "Shown beside the logo where an app asks to connect. A language without one shows the English title, then the name.",
            )}
          </p>
          {NAMES.map(({ lang, native }) => (
            <Input
              key={lang}
              label={native}
              lang={lang}
              value={form.titles[lang]}
              autoComplete="off"
              onChange={(e) => setTitle(lang, e.target.value)}
            />
          ))}
        </div>
        <div className="stack">
          <h3 className="overline">{t("Product pages")}</h3>
          <Switch
            label={t("Hide the Brand book and the Guide")}
            checked={form.hideDocs}
            onChange={(e) => setText({ hideDocs: e.target.checked })}
          />
          <p className="hint">
            {t(
              "They describe the product, not this instance. Hidden for everyone, administrators included. Only people who are signed in can read this setting.",
            )}
          </p>
        </div>
        {error && (
          <Notice tone="danger" title={t("The identity was not saved")}>
            <span className="identity-refusal">{error}</span>
          </Notice>
        )}
        <div className="actionrow">
          <Button
            variant="primary"
            type="submit"
            loading={busy}
            disabled={!dirty || needsName || busy}
          >
            {t("Save")}
          </Button>
          <Button
            variant="ghost"
            disabled={!dirty || busy}
            onClick={() => {
              setForm(loaded.form);
              setLogo({ kind: "keep" });
              setError("");
              setFileError("");
            }}
          >
            {t("Discard changes")}
          </Button>
          {loaded.set && (
            <Button variant="danger" disabled={busy} onClick={() => void remove()}>
              {t("Remove the identity")}
            </Button>
          )}
        </div>
        {needsName && dirty && (
          <p className="hint">{t("A name is needed the first time you save.")}</p>
        )}
      </div>
      <div className="stack">
        <div className="stack identity-preview">
          <h3 className="overline">{t("Preview")}</h3>
          <Select
            label={t("Language of the message in the preview")}
            value={previewLang}
            onChange={(e) => setPreviewLang(e.target.value as Lang)}
          >
            {NAMES.map(({ lang, native }) => (
              <option key={lang} value={lang}>
                {native}
              </option>
            ))}
          </Select>
          <div
            className="identity-card"
            lang={previewLang}
            role="group"
            aria-label={t("Preview of the sign-in screen")}
          >
            <InstanceLogo brand={preview} src={cardSrc} variant="full" size={30} />
            <p className="identity-card-title">{t("Sign in")}</p>
            <p className="hint">{t("Sign in to your instance.")}</p>
            {cardMessage.text && (
              <p
                className="hint"
                lang={cardMessage.lang === previewLang ? undefined : cardMessage.lang}
              >
                {cardMessage.text}
              </p>
            )}
            <span className="btn primary large identity-card-button" aria-hidden="true">
              {t("Sign in")}
            </span>
          </div>
          <div className="identity-tile">
            {loaded.set && !removing ? (
              <img
                src={logo.kind === "replace" && pending ? pending : brandLogoUrl(preview)}
                alt=""
                width={64}
                height={64}
              />
            ) : (
              <span className="identity-tile-empty" aria-hidden="true" />
            )}
            <p className="hint">
              {loaded.set
                ? t("The tile that stands for the instance in the tab and on the pages of apps.")
                : t("Shown after the first save.")}
            </p>
          </div>
        </div>
      </div>
    </form>
  );
}
