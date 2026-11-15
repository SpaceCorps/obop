// extensions.ts and the registrations that moved out of RecordPanel, App and ModelPage (F3).
import { describe, expect, it } from "vite-plus/test";
import type { App } from "./Chrome.tsx";
import {
  appHomeOf,
  appPages,
  modelPageOf,
  recordExtensionOf,
  registerRecordExtension,
} from "./extensions.ts";
import "./registrations.ts";

const app = (id: string, models: string[]): App => ({
  id,
  name: id,
  color: "",
  module: {
    id,
    name: id,
    models: models.map((name) => ({
      name,
      description: "",
      can_create: false,
      can_update: false,
      can_delete: false,
      fields: [],
      actions: [],
    })),
  },
});

describe("registrations", () => {
  it("are loaded from src/registrations", () => {
    expect(recordExtensionOf("sign_request").actions).toHaveLength(1);
    expect(recordExtensionOf("sign_request").panels).toHaveLength(2);
    expect(modelPageOf("documents_document")).toBeTypeOf("function");
    expect(Object.keys(appPages).sort()).toEqual(["connectors", "dashboards", "documents"]);
  });
  it("a model without one gets empty lists", () => {
    expect(recordExtensionOf("lead")).toEqual({ actions: [], panels: [], sections: [] });
    expect(modelPageOf("lead")).toBeUndefined();
  });
  it("merges registrations for one model in order", () => {
    const A = () => null;
    const B = () => null;
    registerRecordExtension("test_model", { sections: [A] });
    registerRecordExtension("test_model", { sections: [B], actions: [A] });
    expect(recordExtensionOf("test_model").sections).toEqual([A, B]);
    expect(recordExtensionOf("test_model").actions).toEqual([A]);
  });
  it("Dashboards opens on its overview only when it has dashboards", () => {
    expect(appHomeOf(app("dashboards", ["dashboards_dashboard"]))).toBe("#/dashboards/board");
    expect(appHomeOf(app("dashboards", ["dashboards_widget"]))).toBeUndefined();
    expect(appHomeOf(app("crm", ["lead"]))).toBeUndefined();
  });
});
