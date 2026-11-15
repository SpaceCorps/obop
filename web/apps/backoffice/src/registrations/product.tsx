// Products: a product shows the units on hand and where, when Inventory is installed.
import { registerRecordExtension } from "../extensions.ts";
import { OnHand } from "../apps/product/OnHand.tsx";

registerRecordExtension("product", { panels: [OnHand] });
