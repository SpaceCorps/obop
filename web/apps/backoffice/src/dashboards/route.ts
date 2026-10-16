import { href } from "../route.ts";

/** The route of the Dashboards overview: `#/dashboards/board?id=…&view=edit`. */
export const BOARD = "board";

/** Where the Dashboards app opens: the overview, not a list of dashboard records. */
export const boardHref = (appId: string) => href({ app: appId, model: BOARD });
