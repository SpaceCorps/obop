// Local preview of the website: assemble the sources once, then serve them with VitePress.
// Content edits need a re-run of `vp run dev` (the assemble step copies and generates the tree).
import { createServer } from "vitepress";
import { assemble } from "./assemble.ts";
import { SITE_ROOT } from "./lib/paths.ts";

assemble();
const server = await createServer(SITE_ROOT, { host: "127.0.0.1", port: 5180 });
await server.listen();
server.printUrls();
