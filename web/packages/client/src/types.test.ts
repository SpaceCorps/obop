import { describe, expect, expectTypeOf, test } from "vite-plus/test";
import {
  createObopClient,
  type BulkModel,
  type BulkReport,
  type BulkRow,
  type ObopClient,
  type ObopMeta,
  type components,
  type paths,
} from "./index.ts";

// These tests are about types. `expectTypeOf` and the `@ts-expect-error` lines are checked by
// `vp check` (an unused `@ts-expect-error` is itself an error), not by running them.
describe("operations are typed from the generated schema", () => {
  type Response200<P extends keyof paths> = paths[P] extends {
    post: { responses: { 200: { content: { "application/json": infer R } } } };
  }
    ? R
    : never;

  test("list answers records of the model and the withheld field names", () => {
    type List = Response200<"/api/party/list">;
    expectTypeOf<NonNullable<List["value"]>[number]>().toEqualTypeOf<
      components["schemas"]["party"]
    >();
    expectTypeOf<List["denied"]>().toEqualTypeOf<string[] | undefined>();
    expectTypeOf<components["schemas"]["party"]["name"]>().toEqualTypeOf<string | undefined>();
  });

  test("search takes a typed filter", () => {
    type Search = NonNullable<
      paths["/api/party/search"]["post"]["requestBody"]
    >["content"]["application/json"];
    expectTypeOf<NonNullable<Search["filter"]>[number]["op"]>().toEqualTypeOf<
      "eq" | "ne" | "contains" | "in" | "gt" | "gte" | "lt" | "lte" | "is_null"
    >();
    expectTypeOf<Search["limit"]>().toEqualTypeOf<number | undefined>();
  });

  test("an error answer is the Error schema, with the D74 word", () => {
    type Err = components["schemas"]["Error"];
    expectTypeOf<Err["applied"]>().toEqualTypeOf<"none" | "unknown" | "partial" | undefined>();
    expectTypeOf<Err["code"]>().toEqualTypeOf<string | undefined>();
    type R503 = paths["/api/party/create"]["post"]["responses"][503];
    expectTypeOf<R503["headers"]["X-Obop-Applied"]>().toEqualTypeOf<
      "none" | "unknown" | "partial" | undefined
    >();
  });

  test("the typed client refuses a path or a body the schema does not have", () => {
    const client = createObopClient({ baseUrl: "https://x.example", getToken: () => null });
    const typeOnly = () => {
      void client.api.POST("/api/party/list");
      void client.api.POST("/api/party/create", { body: { name: "Ada" } });
      // @ts-expect-error no such path
      void client.api.POST("/api/nothing/list");
      // @ts-expect-error `name` is required to create a party
      void client.api.POST("/api/party/create", { body: { email: "ada@example.com" } });
      // @ts-expect-error `name` is a string
      void client.api.POST("/api/party/create", { body: { name: 3 } });
    };
    expect(typeOnly).toBeTypeOf("function");
  });
});

describe("bulkCreate is typed from the schema", () => {
  test("the models are those that have bulk_create", () => {
    expectTypeOf<"party">().toExtend<BulkModel>();
    expectTypeOf<"product">().toExtend<BulkModel>();
    expectTypeOf<"not_a_model">().not.toExtend<BulkModel>();
    expectTypeOf<"mileage_trip">().not.toExtend<BulkModel>();
  });

  test("a row is the writable fields of the model", () => {
    expectTypeOf<BulkRow<"party">["name"]>().toEqualTypeOf<string>();
    expectTypeOf<BulkRow<"party">["email"]>().toEqualTypeOf<string | undefined>();
    expectTypeOf<BulkRow<"party">>().not.toHaveProperty("id");
    // bulk_create refuses an image or file field (a blob is uploaded and claimed one record at a
    // time), so a row does not offer one, though create does: the schema is the source of this.
    expectTypeOf<BulkRow<"party">>().not.toHaveProperty("photo");
    expectTypeOf<components["schemas"]["party"]>().toHaveProperty("photo");
  });

  test("the report says whether it is a replay", () => {
    expectTypeOf<BulkReport["replayed"]>().toEqualTypeOf<boolean | undefined>();
    expectTypeOf<BulkReport["created"]>().toEqualTypeOf<number | undefined>();
    expectTypeOf<BulkReport["ids"]>().toEqualTypeOf<(string | null)[] | undefined>();
    expectTypeOf<NonNullable<BulkReport["errors"]>[number]["row"]>().toEqualTypeOf<
      number | undefined
    >();
  });

  test("the call is checked against the model's fields", () => {
    const client = createObopClient({ baseUrl: "https://x.example", getToken: () => null });
    const typeOnly = () => {
      void client.bulkCreate("party", [{ name: "Ada", email: "ada@example.com" }], {
        idempotencyKey: "k",
        mode: "partial",
        dryRun: true,
      });
      // @ts-expect-error `name` is required
      void client.bulkCreate("party", [{ email: "ada@example.com" }]);
      // @ts-expect-error the schema has no such field
      void client.bulkCreate("party", [{ name: "Ada", nope: 1 }]);
      // @ts-expect-error an image field is a blob id and bulk_create always refuses it
      void client.bulkCreate("party", [{ name: "Ada", photo: "a".repeat(64) }]);
      // @ts-expect-error a mode that does not exist
      void client.bulkCreate("party", [{ name: "Ada" }], { mode: "some" });
      // A model of a package installed later is not in the schema: untyped rows.
      void client.bulkCreate("mileage_trip", [{ client_trip_id: "t", anything: 1 }]);
      // A model name that is only known at run time is untyped too.
      const model: string = "party";
      void client.bulkCreate(model, [{ whatever: true }]);
    };
    expect(typeOnly).toBeTypeOf("function");
  });
});

describe("getMeta", () => {
  test("answers the loose metadata type", () => {
    expectTypeOf<ObopClient["getMeta"]>().returns.resolves.toEqualTypeOf<ObopMeta>();
    expectTypeOf<
      ObopMeta["modules"][number]["models"][number]["fields"][number]["kind"]
    >().toBeString();
  });
});
