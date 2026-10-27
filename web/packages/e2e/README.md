# @obop/e2e

End-to-end tests: Playwright drives the real built app in a real browser. Nothing is mocked; every run
starts a fresh instance (new temp directory, new SQLite files, free ports) and removes it afterwards.

## Run it

From the repository root, build what is under test once, then run the suite:

```sh
cargo build                              # target/debug/obop (or point OBOP_BIN at another binary)
(cd web/apps/backoffice && vp build)     # web/apps/backoffice/dist
deploy/build-site.sh                     # web/site: the website + sign-up app
cd web/packages/e2e && vp run e2e        # the suite (vp run e2e:ui for Playwright's UI mode)
```

Pass Playwright arguments after the script name, for example `vp run e2e tests/kanban.spec.ts -g drag`
or `vp run e2e --repeat-each=5`. Failed tests keep a trace and screenshot in `test-results/`; in CI the
HTML report goes to `playwright-report/`. `OBOP_E2E_KEEP=1` keeps the temp directory with the databases.

## Browser

- Locally the suite uses the system Chrome (`channel: "chrome"`), because Playwright's own browsers are
  not downloaded. `PW_CHROME_PATH=/path/to/chrome` uses a specific executable
  (macOS: `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`); `PW_CHANNEL=msedge` picks another channel.
- With `CI` set it uses Playwright's bundled Chromium. Install it with
  `vp exec playwright install --with-deps chromium`.

## How it is built

- `playwright.config.ts` picks the temp directory and ports, and declares two `webServer`s. Playwright
  starts web servers before `globalSetup`, so each one is launched through `support/serve.ts`, which first
  creates the accounts with the CLI (`obop admin create`, password on stdin) and then starts the server:
  `obop serve --static web/apps/backoffice/dist` with admin `root` (group `platform.admin`) and restricted
  user `sam` (`base.user,crm.user,sales.user,product.user`), and `obop platform serve --provisioner fake --static web/site` for the control plane.
- One instance backs the whole run, so specs run one at a time. Tests that assert counts or totals install a
  throwaway module (`scratchModule`) and so own all of its data; every other test uses uniquely named
  records (`uid`) and restores anything instance-wide it changes (theme, installed modules).
- `support/fixtures.ts`: `signIn(user)` (API login, then open the app), `api` (REST client), `scratchModule`,
  and an automatic guard that fails any test with a page error or console error. Chrome logs every 4xx/5xx
  response as a console error, so a test that provokes one on purpose declares it with `allowHttpErrors(400)`.
- Drag and drop: a pointer drag needs the card and the drop target on screen together, so tests drop on a
  column header. See the "tall board" test in `tests/kanban.spec.ts` for the synthetic-event variant.
- Redirects back to a client: `page.route()` is never called for a request that is a redirect target (Playwright
  sends it on to the network), so a test that needs the browser's redirect to a loopback client (the OAuth specs)
  starts a real listener with `support/callback.ts` and registers its address, instead of routing an invented one.
  A route-based version passes on a desktop Chrome, which reloads its error page, and times out in CI's headless shell.
- `test.fixme` marks a known application bug with its reproduction in a comment; fix the app, then remove it.

## Languages

The suite pins English: the `language` option (default `en`) stores `obop-lang` before any page script
runs, and `locale` is `en-US`. `tests/i18n.spec.ts` opts out per test (`test.use({ language: null })`) or
opens a language with `?lang=de`. It reads the expected wording from the catalogues in
`web/apps/*/locales/<lang>/messages.json`, so a reworded translation does not break it.
`tests/default-language.spec.ts` opens a further instance (`serve.ts language`, its own port and database, only
`root` and `sam`) that starts with `OBOP_DEFAULT_LANGUAGE=sv`, in a browser that asks for English: the screens
before sign-in must speak Swedish. The launcher clears the variable for every other instance, so a value in
your shell cannot turn their English into something else.
`support/layout.ts` finds sideways scroll, controls outside the viewport and cut-off text from element boxes.

## The QA sweep

`tests/qa-sweep.spec.ts` is a manual-style pass, skipped unless `OBOP_QA=1`: it visits every screen at
1440, 1024, 768 and 390 px, light and dark, in English and German, reports layout problems and saves a
screenshot of each to `OBOP_QA_OUT` (default `/tmp/obop-qa`); the console guard fails it on any console or page error.
