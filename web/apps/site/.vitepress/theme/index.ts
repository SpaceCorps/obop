// The type check of `vp check` reads each file on its own: this brings in the module declarations
// for .vue and .css imports (env.d.ts), which `tsc -p .` takes from the tsconfig.
// oxlint-disable-next-line typescript/triple-slash-reference
/// <reference path="../../env.d.ts" />
import type { Theme } from "vitepress";
import Layout from "./Layout.vue";
import "./style.css";
import "./apps.css";
import "./app-page.css";

export default { Layout } satisfies Theme;
