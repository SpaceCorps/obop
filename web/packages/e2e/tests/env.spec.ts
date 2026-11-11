// The check of the /admin/ build that the config runs before anything starts (support/env.ts,
// `adminBuildProblem`): a page that does not load from /admin/assets/ is refused with the way out,
// instead of a blank screen and one failing test that hides the rest.
import { expect, test } from "@playwright/test";
import { adminBuildProblem } from "../support/env.ts";

/** A page as `vp build` writes it with `OBOP_ADMIN_BASE=/admin/`. */
const page = (base: string, extra = "") => `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" href="${base}favicon.svg" type="image/svg+xml" />
    <title>obop</title>
    <script type="module" crossorigin src="${base}assets/index-BR9epSWn.js"></script>
    <link rel="modulepreload" crossorigin href="${base}assets/route-Dg2pOSh0.js">
    <link rel="modulepreload" crossorigin href="${base}assets/ui-2NfLOVXv.js">
    <link rel="stylesheet" crossorigin href="${base}assets/index-BYHjWGyx.css">
  </head>
  <body>
    <div id="root"></div>${extra}
  </body>
</html>
`;

const WAY_OUT = /MSYS_NO_PATHCONV=1/;

test.describe("the check of the /admin/ build", () => {
  test("a page that loads every file from /admin/assets/ is accepted", () => {
    expect(adminBuildProblem(page("/admin/"))).toBeNull();
  });

  test("a page that Git Bash on Windows rewrote is refused, and the way out is named", () => {
    const problem = adminBuildProblem(page("/Program Files/Git/admin/"));
    expect(problem).toMatch(/loads \/Program Files\/Git\/admin\/assets\/index-BR9epSWn\.js/);
    expect(problem).toMatch(WAY_OUT);
    // A drive letter in front is the same rewriting.
    expect(adminBuildProblem(page("C:/Program Files/Git/admin/"))).toMatch(WAY_OUT);
  });

  test("a page built for the root is refused", () => {
    const problem = adminBuildProblem(page("/"));
    expect(problem).toMatch(/loads \/assets\/index-BR9epSWn\.js/);
    expect(problem).toMatch(WAY_OUT);
  });

  test("one good reference does not make a page right: every one into assets/ must be", () => {
    const good = page("/admin/");
    // The script is wrong and the modulepreload links are right.
    const script = good.replace(
      'src="/admin/assets/index-BR9epSWn.js"',
      'src="/Program Files/Git/admin/assets/index-BR9epSWn.js"',
    );
    expect(script).not.toBe(good);
    expect(adminBuildProblem(script)).toMatch(/loads \/Program Files\/Git\/admin\/assets\/index/);
    // The script is right and one link is wrong.
    const link = good.replace(
      'href="/admin/assets/ui-2NfLOVXv.js"',
      'href="/assets/ui-2NfLOVXv.js"',
    );
    expect(link).not.toBe(good);
    expect(adminBuildProblem(link)).toMatch(/loads \/assets\/ui-2NfLOVXv\.js/);
    const style = good.replace('href="/admin/assets/index-BYHjWGyx.css"', 'href="assets/x.css"');
    expect(style).not.toBe(good);
    expect(adminBuildProblem(style)).toMatch(/loads assets\/x\.css/);
  });

  test("the string in a piece of text is not a reference", () => {
    const text = '<p>src="/admin/assets/index.js" and href="/admin/assets/index.css"</p>';
    expect(adminBuildProblem(`<!doctype html><html><body>${text}</body></html>`)).toMatch(
      /has no script from \/admin\/assets\//,
    );
    // Nor does it hide a wrong script behind a good-looking piece of text.
    expect(adminBuildProblem(page("/", text))).toMatch(/loads \/assets\/index-BR9epSWn\.js/);
  });

  test("a page without any script is refused", () => {
    expect(adminBuildProblem("<!doctype html><html><body></body></html>")).toMatch(WAY_OUT);
    expect(adminBuildProblem("")).toMatch(WAY_OUT);
  });
});
