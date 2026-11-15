// extensions.ts and the registrations that moved out of RecordPanel, App and ModelPage (F3).
import { describe, expect, it } from "vite-plus/test";
import type { App } from "./Chrome.tsx";
import {
  appHomeOf,
  appPages,
  modelPageOf,
  recordExtensionOf,
  registerRecordExtension,
  withoutOverridden,
} from "./extensions.ts";
import type { Command } from "./api.ts";
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
    // Every app's registration file adds to this; the test names some, not all of them.
    expect(Object.keys(appPages)).toEqual(
      expect.arrayContaining(["connectors", "dashboards", "documents"]),
    );
  });
  it("a model without one gets empty lists", () => {
    expect(recordExtensionOf("lead")).toEqual({
      actions: [],
      panels: [],
      sections: [],
      overrides: [],
    });
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
  it("an action an extension draws itself is left out of the next actions", () => {
    const cmd = (id: string): Command => ({ id, description: "", needs_approval: false });
    const [receive, cancel, send] = [cmd("po.receive"), cmd("po.cancel"), cmd("po.send")];
    const next = { primary: receive, secondary: [send], danger: [cancel] };
    expect(withoutOverridden(next, [])).toBe(next);
    expect(withoutOverridden(next, ["po.receive", "po.cancel"])).toEqual({
      secondary: [send],
      danger: [],
    });
  });
  it("Dashboards opens on its overview only when it has dashboards", () => {
    expect(appHomeOf(app("dashboards", ["dashboards_dashboard"]))).toBe("#/dashboards/board");
    expect(appHomeOf(app("dashboards", ["dashboards_widget"]))).toBeUndefined();
    expect(appHomeOf(app("crm", ["lead"]))).toBeUndefined();
  });
});
