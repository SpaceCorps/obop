/**
 * Which component draws which kind of view, with its name and icon in the view switcher. ModelPage
 * draws the view of the route through `viewOf`; a lane that adds a view kind adds a line here and
 * its component file. Which kinds a model offers is ModelPage's choice (`available`), not this file's.
 */
import type { ComponentType } from "react";
import { msg, type IconName } from "@obop/ui";
import { CalendarView } from "./CalendarView.tsx";
import { GalleryView } from "./GalleryView.tsx";
import { GraphView } from "./GraphView.tsx";
import { GridView } from "./GridView.tsx";
import { KanbanView } from "./KanbanView.tsx";
import { ListView } from "./ListView.tsx";
import { PivotView } from "./PivotView.tsx";
import type { ViewProps } from "./types.ts";

export type ViewKind = "list" | "kanban" | "calendar" | "pivot" | "graph" | "gallery" | "grid";

export type ViewEntry = {
  id: ViewKind;
  /** The English name, marked for translation (`msg`); the switcher translates it when drawn. */
  label: string;
  icon: IconName;
  component: ComponentType<ViewProps>;
};

export const VIEW_REGISTRY: Record<ViewKind, ViewEntry> = {
  list: { id: "list", label: msg("List"), icon: "list", component: ListView },
  kanban: { id: "kanban", label: msg("Kanban"), icon: "kanban", component: KanbanView },
  calendar: { id: "calendar", label: msg("Calendar"), icon: "calendar", component: CalendarView },
  pivot: { id: "pivot", label: msg("Pivot"), icon: "pivot", component: PivotView },
  graph: { id: "graph", label: msg("Graph"), icon: "graph", component: GraphView },
  gallery: { id: "gallery", label: msg("Gallery"), icon: "grid", component: GalleryView },
  // The week grid (Timesheets): offered only where a package asks for it (`default_view = "grid"`).
  grid: { id: "grid", label: msg("Week"), icon: "clock", component: GridView },
};

export const isViewKind = (v: string): v is ViewKind => Object.hasOwn(VIEW_REGISTRY, v);

/** The entry of a view kind; an unknown kind is the list. */
export function viewOf(kind: string): ViewEntry {
  return isViewKind(kind) ? VIEW_REGISTRY[kind] : VIEW_REGISTRY.list;
}
