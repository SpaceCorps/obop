import {
  test as base,
  expect,
  type APIRequestContext,
  type ConsoleMessage,
  type Page,
} from "@playwright/test";
import { users, type UserName } from "./env.ts";
import { manifest, type ModuleSpec } from "./modules.ts";

export { expect };

export type Reply = { status: number; json: any };

/** A thin client for the REST API, used to arrange data and to check what the server enforces. */
export class Api {
  private tokens = new Map<UserName, string>();
  constructor(private request: APIRequestContext) {}

  async post(path: string, body: object = {}, token?: string): Promise<Reply> {
    const res = await this.request.post(path, {
      data: body,
      headers: token ? { authorization: `Bearer ${token}` } : {},
    });
    return { status: res.status(), json: await res.json().catch(() => null) };
  }

  async get(path: string, token?: string): Promise<Reply> {
    const res = await this.request.get(path, {
      headers: token ? { authorization: `Bearer ${token}` } : {},
    });
    return { status: res.status(), json: await res.json().catch(() => null) };
  }

  async token(user: UserName): Promise<string> {
    const known = this.tokens.get(user);
    if (known) return known;
    const r = await this.post("/api/auth/login", {
      username: users[user].name,
      password: users[user].password,
    });
    expect(r.status, `login as ${user}`).toBe(200);
    this.tokens.set(user, r.json.token);
    return r.json.token;
  }

  /** Run a command as `user` and return the value; fails the test on any non-200 reply. */
  async run(user: UserName, id: string, body: object = {}): Promise<any> {
    const [model, command] = id.split(".");
    const r = await this.post(`/api/${model}/${command}`, body, await this.token(user));
    expect(r.status, `${id} as ${user}: ${JSON.stringify(r.json)}`).toBe(200);
    return r.json.value;
  }

  /** Create a record as `user`; returns it. */
  create(user: UserName, model: string, body: object): Promise<any> {
    return this.run(user, `${model}.create`, body);
  }

  list(user: UserName, model: string): Promise<any[]> {
    return this.run(user, `${model}.list`);
  }
}

/** Test options: `test.use({ language: "de" })`; `null` leaves the choice to the browser's language. */
type Options = {
  /** The interface language every context starts with. The suite pins English unless a spec says otherwise. */
  language: string | null;
};

type Fixtures = {
  api: Api;
  /** Sign in as a test user through the API and open the app (the sign-in form has its own spec). */
  signIn: (user: UserName, hash?: string) => Promise<void>;
  /** Declare the HTTP error statuses this test provokes on purpose; any others fail the test. */
  allowHttpErrors: (...statuses: number[]) => void;
  /** Declare console errors this test provokes on purpose (a dropped connection, a crash caught by the error boundary). */
  allowConsoleErrors: (...patterns: RegExp[]) => void;
  /** Install a throwaway module as root (removed after the test); returns its model name. */
  scratchModule: (spec: Omit<ModuleSpec, "id"> & { id?: string }) => Promise<string>;
  /**
   * Install a throwaway package written as a whole `obop.toml` (several models, labels...) as root,
   * removed after the test. `build` gets the module id, which every model name must start with.
   */
  scratchPackage: (build: (id: string) => string) => Promise<string>;
  /** A name no other record has, so tests never depend on how much data earlier tests left. */
  uid: (prefix: string) => string;
  /** Collects page errors and console errors and fails the test on any (auto fixture). */
  consoleGuard: void;
  /** Stores the pinned interface language before any page script runs (auto fixture). */
  pinLanguage: void;
};

let counter = 0;

export const test = base.extend<Fixtures & Options>({
  language: ["en", { option: true }],

  pinLanguage: [
    async ({ context, language }, use) => {
      // A saved choice wins over the browser's language, so the suite never depends on the machine.
      if (language)
        await context.addInitScript((l) => {
          try {
            if (!localStorage.getItem("obop-lang")) localStorage.setItem("obop-lang", l);
          } catch {
            /* storage unavailable */
          }
        }, language);
      await use();
    },
    { auto: true },
  ],

  api: async ({ request }, use) => use(new Api(request)),

  // eslint-disable-next-line no-empty-pattern -- Playwright needs a destructured first argument
  uid: async ({}, use, info) => {
    const run = Date.now().toString(36);
    await use((prefix) => `${prefix} ${run}${++counter}w${info.workerIndex}`);
  },

  scratchModule: async ({ api }, use) => {
    const installed: string[] = [];
    await use(async (spec) => {
      const id = spec.id ?? `t${Date.now().toString(36)}${++counter}`;
      await api.run("root", "module.install", { manifest: manifest({ ...spec, id }) });
      installed.push(id);
      return id;
    });
    for (const id of installed) await api.run("root", "module.uninstall", { id });
  },

  scratchPackage: async ({ api }, use) => {
    const installed: string[] = [];
    await use(async (build) => {
      const id = `t${Date.now().toString(36)}${++counter}`;
      await api.run("root", "module.install", { manifest: build(id) });
      installed.push(id);
      return id;
    });
    for (const id of installed) await api.run("root", "module.uninstall", { id });
  },

  signIn: async ({ page, api }, use) => {
    await use(async (user, hash = "") => {
      const token = await api.token(user);
      await page.addInitScript(
        ([t, u]) => {
          sessionStorage.setItem("obop-token", t!);
          sessionStorage.setItem("obop-user", u!);
        },
        [token, users[user].name],
      );
      await page.goto(`/${hash ? `#${hash}` : ""}`);
      await expect(page.locator(".topbar")).toBeVisible();
    });
  },

  // eslint-disable-next-line no-empty-pattern -- Playwright needs a destructured first argument
  allowHttpErrors: async ({}, use) => {
    // The guard (an auto fixture) reads the module-level list and resets it before each test.
    await use((...statuses) => void allowed.push(...statuses));
  },

  // eslint-disable-next-line no-empty-pattern -- Playwright needs a destructured first argument
  allowConsoleErrors: async ({}, use) => {
    await use((...patterns) => void allowedConsole.push(...patterns));
  },

  consoleGuard: [
    async ({ page }, use, info) => {
      const problems: string[] = [];
      const failedLoads: { status: number; text: string }[] = [];
      allowed.length = 0;
      allowedConsole.length = 0;
      page.on("pageerror", (e) => {
        if (!allowedConsole.some((p) => p.test(e.message)))
          problems.push(`page error: ${e.message}`);
      });
      page.on("console", (m: ConsoleMessage) => {
        if (m.type() !== "error") return;
        // Chrome logs every 4xx/5xx response as a console error; the test declares the expected ones.
        const http = /Failed to load resource: the server responded with a status of (\d+)/.exec(
          m.text(),
        );
        if (http) failedLoads.push({ status: Number(http[1]), text: m.text() });
        else if (!allowedConsole.some((p) => p.test(m.text())))
          problems.push(`console error: ${m.text()}`);
      });
      await use();
      if (info.status !== info.expectedStatus) return; // the test already failed for its own reason
      for (const f of failedLoads)
        if (!allowed.includes(f.status)) problems.push(`unexpected ${f.text}`);
      expect(problems, "console errors and page errors").toEqual([]);
    },
    { auto: true },
  ],
});

const allowed: number[] = [];
const allowedConsole: RegExp[] = [];

/** Open a hash route in the already signed-in app and wait for the page it names. */
export async function open(page: Page, hash: string): Promise<void> {
  await page.goto(`/#${hash}`);
  await expect(page.locator(".topbar")).toBeVisible();
}

/** Every visible control must have an accessible name; returns the offenders. */
export async function unnamedControls(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const bad: string[] = [];
    const sel =
      "button, input, select, textarea, a[href], [role=button], [role=combobox], [role=checkbox], [role=radio], [role=tab]";
    for (const el of document.querySelectorAll<HTMLElement>(sel)) {
      if (el instanceof HTMLInputElement && el.type === "hidden") continue;
      if (el.closest("[hidden], [aria-hidden=true]")) continue;
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden") continue;
      const byIds = (el.getAttribute("aria-labelledby") ?? "")
        .split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent?.trim() ?? "")
        .join("");
      const forLabel = el.id ? document.querySelector(`label[for="${CSS.escape(el.id)}"]`) : null;
      const name =
        el.getAttribute("aria-label")?.trim() ||
        byIds ||
        (el instanceof HTMLInputElement ||
        el instanceof HTMLSelectElement ||
        el instanceof HTMLTextAreaElement
          ? (el.labels && el.labels.length
              ? Array.from(el.labels)
                  .map((l) => l.textContent)
                  .join("")
                  .trim()
              : "") || forLabel?.textContent?.trim()
          : "") ||
        (el instanceof HTMLInputElement && el.type === "color" ? el.title : "") ||
        (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement
          ? ""
          : el.textContent?.trim()) ||
        el.getAttribute("title")?.trim();
      if (!name) bad.push(el.outerHTML.slice(0, 140));
    }
    return bad;
  });
}
