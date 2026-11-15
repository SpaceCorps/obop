// The gallery view: cards with the record's image as a cover and no grouping (Products, Contacts,
// Listings). A stub for now (F2 of the UI plan): it draws the list, so nothing is lost while lane L3
// builds the cards.
import { ListView } from "./ListView.tsx";
import type { ViewProps } from "./types.ts";

export function GalleryView(p: ViewProps) {
  return <ListView {...p} />;
}
