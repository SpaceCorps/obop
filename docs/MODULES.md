# How to write a module

Audience: a developer or a business analyst who wants to add an app to obop without writing Rust. Everything here is checked against the code (`crates/kernel/obop-pkg/src/manifest.rs`, `installer.rs`, `marketplace.rs` and the command layer in `obop-commands`). The tutorial module is a real file, `docs/modules/assets.toml`, and the end-to-end test `the_tutorial_module_and_its_documented_errors_are_real` in `crates/obop/src/main.rs` installs it and triggers every error quoted in section 3.5.

## 1. What a module is

A module (also called a package) is one text file, `obop.toml`. It declares **models** (record types), their **fields** (each with a kind, a data class, a description and a permission group) and **actions** (state transitions). Installing it registers the models with the one command dispatcher, so the same module is immediately available as REST endpoints, MCP tools for agents, CLI commands and backoffice screens (list, kanban, form, status bar, buttons), with masking, permission checks and agent approval applied for free (D29, D41, D45, D46).

### Tiers

| Tier | What it can contain | Status |
|---|---|---|
| X1 declarative | Models, typed fields, data classes, permission groups, state-transition actions. No code. | Built (this document; D45, D47) |
| X2 CEL | Expressions stored in data: computed fields, validation, conditions, modifiers, policies. | Not built (D11) |
| X3 Wasm | Sandboxed components for logic that expressions cannot express (connectors, side effects). | Not built (D11) |

X1 packages cannot run anything, so an administrator can install one from an upload at runtime. When X2 and X3 arrive a package will gain sections for expressions and components; the X1 sections described here stay valid. Behaviour that needs code (computed totals, sequences, side effects, validation across fields) waits for those tiers (D45 consequences).

### What ships

Built-in modules (Rust crates, not removable): `base` (model `party`), `product` (`product`), `crm`, `sales` (`sale_order`), `invoicing` (`invoice`). Bundled packages in `packages/*.toml` (installed from the Customize app or `obop modules install-from ID`):

| Package | Needs | What it holds |
|---|---|---|
| `accounting` | base | Chart of accounts, journals, taxes (basis points), fiscal years and periods, journal entries and lines, bank statement lines. Posting and closing need approval and `accounting.approver` |
| `appointments`, `documents`, `expenses`, `fleet`, `helpdesk`, `manufacturing`, `projects`, `purchase`, `inventory`, `subscriptions`, `timesheets`, `hr` | varies | Services, supply chain and people apps |
| `planning` | hr | Roles, shift templates, shifts (open, published, confirmed) |
| `field_service` | base, product, hr | Jobs with site, technician, parts used, signature flag |
| `pos` | base, product | Point-of-sale config, cash-controlled sessions, orders, lines, payments; refunds and session close need approval |
| `ecommerce` | base, product | Listings, shipping methods, discount codes, web orders and lines; refunds need approval |
| `website` | none | Pages, menu items, blog posts, form submissions |
| `email_marketing` | none | Lists, contacts with consent, campaigns with statistics; sending needs approval |
| `sign` | none | Templates and signature requests |
| `dashboards` | none | Dashboards and widgets as data (schema in section 9) |
| `knowledge` | none | Article tree |

## 2. Tutorial: an asset register

We build `assets`: categories of equipment and the individual assets, with a holder, a life cycle and a retire step that needs approval. The full file is `docs/modules/assets.toml`. It is shown in pieces; the pieces concatenated are the file.

### 2.1 The `[module]` table

```toml
[module]
id = "assets"
version = "0.1.0"
name = "Assets"
description = "A register of company equipment: categories, individual assets with a holder, and a stock, assigned, in repair, retired life cycle. Purchase prices are visible to managers only; retiring an asset needs a human approval."
author = "Example Ltd"
depends = ["base"]
```

`id` names the module and prefixes everything it owns: every model must be called `assets` or `assets_<something>`, and by convention permission groups are `assets.<role>`. `version` is three numbers (`0.1.0`); a later install of the same id must have a higher version. `name` is what the app launcher shows. `description` is shown in the marketplace and Installed list, and is required. `depends` lists modules that must already be installed; because `assets_asset` has a ref to `party` (a built-in model of `base`), we depend on `base`.

### 2.2 A simple model

```toml
[[model]]
name = "assets_category"
description = "A kind of asset, for example Laptops or Vehicles"
read_group = "assets.user"
write_group = "assets.manager"

  [[model.field]]
  name = "name"
  description = "Category name"
  class = "none"
  required = true

  [[model.field]]
  name = "useful_life_years"
  description = "How many years assets of this kind are expected to last"
  class = "none"
  kind = "number"
```

A model needs a description and two groups: people holding `read_group` can read records (and `get`/`list`); people holding `write_group` can `create` and `update`. Every field needs a description and a data class. `class = "none"` is how you say "this is not personal data and not otherwise sensitive"; it must be written out. `required = true` makes the server refuse a create without the field. Fields default to `kind = "text"`.

Each model automatically gets four commands: `<model>.get`, `.list`, `.create`, `.update` (there is no delete). Records get an `id` assigned by the server.

### 2.3 The main model: kinds, refs, a field-level group, personal data

```toml
[[model]]
name = "assets_asset"
description = "One piece of equipment the company owns"
read_group = "assets.user"
write_group = "assets.user"

  [[model.field]]
  name = "tag"
  description = "Inventory tag stuck on the asset, unique by convention"
  class = "none"
  required = true

  [[model.field]]
  name = "name"
  description = "What the asset is, for example MacBook Pro 14"
  class = "none"
  required = true

  [[model.field]]
  name = "category_id"
  description = "The category of the asset"
  class = "none"
  kind = "ref"
  to = "assets_category"

  [[model.field]]
  name = "supplier_id"
  description = "The contact the asset was bought from"
  class = "none"
  kind = "ref"
  to = "party"

  [[model.field]]
  name = "replaces_id"
  description = "The older asset this one replaced"
  class = "none"
  kind = "ref"
  to = "assets_asset"

  [[model.field]]
  name = "purchased_on"
  description = "Purchase date (YYYY-MM-DD)"
  class = "none"
  kind = "date"

  [[model.field]]
  name = "purchase_price"
  description = "Price paid (minor currency units, e.g. cents)"
  class = "none"
  kind = "money"
  group = "assets.manager"

  [[model.field]]
  name = "holder"
  description = "Name of the person who has the asset"
  class = "internal"
  personal_data = true

  [[model.field]]
  name = "notes"
  description = "Free-text remarks"
  class = "none"
  kind = "longtext"

  [[model.field]]
  name = "state"
  description = "Life cycle: in_stock, assigned, in_repair, retired"
  class = "none"
  kind = "select"
  options = ["in_stock", "assigned", "in_repair", "retired"]
  required = true
```

Things to notice:

- `category_id` refs a model of the same package (`assets_category`); it is checked when the file is parsed. `supplier_id` refs `party`, which belongs to `base`: that is why `depends = ["base"]`, and the installer also checks at install time that `party` is registered. `replaces_id` points at its own model, which is allowed (a ref to itself is optional here; it could not be `required`, because the first record could never be created).
- `purchase_price` has `group = "assets.manager"`. A person who holds only `assets.user` does not see this field at all (it is absent from `/api/meta` and from reads). The default group of a field is the model's `read_group`.
- `holder` is a person's name, so `personal_data = true` and the class cannot be `none`. `internal` ("non-public business data about people and work") fits.
- `state` is the conventional status field. For a `select` called `state` the options are the status order, and the UI draws them as a status bar and kanban columns.

### 2.4 Actions

```toml
  [[model.action]]
  name = "assign"
  description = "Hand the asset to a person"
  field = "state"
  from = "in_stock"
  to = "assigned"

  [[model.action]]
  name = "take_back"
  description = "Take the asset back into stock"
  field = "state"
  from = "assigned"
  to = "in_stock"

  [[model.action]]
  name = "send_to_repair"
  description = "Send an assigned asset to repair"
  field = "state"
  from = "assigned"
  to = "in_repair"

  [[model.action]]
  name = "retire"
  description = "Write the asset off for good (manager only)"
  field = "state"
  from = "in_stock"
  to = "retired"
  group = "assets.manager"
  approval = true
```

Actions belong to the `[[model]]` above them (they are written after its fields). `asset.assign` takes `{ "id": ... }`, checks that `state` is `in_stock` and sets it to `assigned`. Called again, it fails: `state must be in_stock to do this, it is "assigned"`. `retire` is restricted to `assets.manager` and has `approval = true`: a human with that group runs it directly, but an AI agent can only *propose* it; a human then approves the proposal (section 6).

### 2.5 Try it

```sh
cargo run -- modules install docs/modules/assets.toml --db sqlite:///tmp/x.db
# installed assets 0.1.0 (models: assets_category, assets_asset; groups: assets.manager, assets.user)
cargo run -- modules list --db sqlite:///tmp/x.db
```

Installing it again fails with `assets 0.1.0 is installed; installing 0.1.0 is not an upgrade`. Edit `version` to `0.2.0` and it upgrades in place. In the backoffice open Customize, Modules, "Upload your own" and pick the file, then find "Assets" in the app launcher. Note that `platform.admin` holds every group, so an admin sees all of it; create a user with `--groups assets.user` to see what an ordinary employee sees.

## 3. Manifest reference

The file is TOML, at most **256 KiB**, with at most **50 models** and **200 fields in total** across all models. Unknown keys anywhere are errors (`invalid obop.toml: unknown field ...`). Identifiers (`id`, model, field and action names, and both halves of a group name) are lowercase ASCII letters, digits and `_`, start with a letter, at most 40 characters.

### 3.1 `[module]` (one, required)

| Key | Type | Required | Rules |
|---|---|---|---|
| `id` | string | yes | Identifier. Must not equal a built-in module id. |
| `version` | string | yes | `major.minor.patch`, three numbers of at most six digits each. |
| `name` | string | yes | Not blank. |
| `description` | string | yes | Not blank. |
| `author` | string | no | Default empty. Shown in listings. |
| `depends` | array of module ids | no | Default `[]`. Each must be installed (built-in or package) when installing; a module that others depend on cannot be uninstalled. |
| `network_destinations` | array | no | Must be empty or absent: declarative packages may not reach the network. The key exists so later tiers share the contract. |

### 3.2 `[[model]]` (any number)

| Key | Type | Required | Rules |
|---|---|---|---|
| `name` | string | yes | Identifier equal to the module id or starting with `<id>_`; unique in the file; must not already exist in the system. |
| `description` | string | yes | Shown in the gate and the UI. |
| `read_group` | string | yes | `module.role`. Needed for `get`, `list`. |
| `write_group` | string | yes | `module.role`. Needed for `create`, `update`. |
| `field` | array of tables | no | See 3.3. |
| `action` | array of tables | no | See 3.4. |

`[[model.field]]` and `[[model.action]]` are written after their model, indented by convention.

### 3.3 `[[model.field]]`

| Key | Type | Default | Rules |
|---|---|---|---|
| `name` | string | required | Identifier, unique in the model, never `id`. |
| `description` | string | required | Not blank (the gate refuses `field has no description`). |
| `class` | string | required | One of `none`, `public_directory`, `internal`, `confidential_hr`, `special_category`, `financial`, `secret`. See section 5. |
| `personal_data` | bool | `false` | `true` if the value is about an identifiable person. Not allowed with `class = "none"`. |
| `group` | string | the model's `read_group` | `module.role`. Principals without it do not see the field. |
| `kind` | string | `text` | `text`, `longtext`, `number`, `money`, `date`, `bool`, `select`, `ref`. |
| `options` | array of strings | none | Only for `select` (then required, non-empty, no blanks, no duplicates). |
| `to` | string | none | Only for `ref` (then required). Model name. |
| `required` | bool | `false` | Must be present and non-null on create; cannot be set to null on update. |

### 3.4 `[[model.action]]`

| Key | Type | Default | Rules |
|---|---|---|---|
| `name` | string | required | Identifier; not `get`, `list`, `create` or `update`. Keep names unique within a model (the manifest check does not catch duplicates). The command id is `<model>.<name>`. |
| `description` | string | required | Not blank. |
| `field` | string | required | A field declared in the same model. Every bundled package and the UI use `state`. |
| `from` | string | none | The value the field must have now. If the field is a `select`, it must be one of its options. Without `from` the action works from any value; the bundled packages always set it. |
| `to` | string | required | The value to set. For a `select` field it must be one of the options. |
| `group` | string | the model's `write_group` | `module.role`. |
| `approval` | bool | `false` | Agents may only propose it (D18). |

### 3.5 Validation errors you can hit

Parse and gate errors (from `obop modules install`, the Customize upload or `module.install`), and how to fix them:

| Message contains | Cause and fix |
|---|---|
| `invalid obop.toml: ...` | Not valid TOML, a missing required key, a value of the wrong type (for example `required = "yes"`), or an unknown key. Read the line and column in the message. |
| `manifest is larger than 256 KiB` | Split the module. |
| `module id "X" must be lowercase letters, digits and _, starting with a letter` | Fix the id. |
| `version must look like 1.2.3` | Three dot-separated numbers. |
| `module needs a name and a description` | Fill them in. |
| `declarative packages run no code and may not declare network destinations` | Remove `network_destinations`. |
| `package is too large` | More than 50 models or 200 fields. |
| `model "X" must be "<id>" or start with "<id>_"` | Rename the model so it carries the module id as prefix. |
| `model X is declared twice` | Duplicate model name. |
| `permission group "X" must look like module.role` | Two identifiers joined by a dot, for example `assets.user`. |
| `M.F: bad or repeated field name` | Not an identifier, named `id`, or declared twice. |
| `M.F: unknown data class "X"` | Use one of the seven classes. |
| `M.F: unknown kind "X" (use text, longtext, number, money, date, bool, select or ref)` | Floats do not exist; use `number` (integer) or `money` (integer minor units). |
| `M.F: options are only valid for kind "select"` / `` `to` is only valid for kind "ref" `` | Remove the stray key or fix the kind. |
| `M.F: a select needs options` / `select options must be non-empty and unique` | Give at least one option; no blanks or duplicates. |
| `M.F: a ref needs `to`, the target model` | Add `to`. |
| `M.F: `to` "X" is not a model name` | Target must be an identifier. |
| `M.F: `to` names "X", which this package does not declare` | A ref to a model that starts with your own id must be one of your models. |
| `M: bad action name "X"` | Not an identifier, or one of `get`, `list`, `create`, `update`. |
| `M.A: acts on undeclared field "X"` | `field` must name a field of the same model. |
| `M.A: "V" is not one of the options of F (...)` | `from` or `to` is not in the select's options. |
| `...: personal data cannot be classed `none`` | The D41 gate: choose a real class for a field with `personal_data = true`. |
| `...: field has no description` / `command has no description` / `model has no description` | Descriptions are mandatory (they are what agents and the UI show). |

Install-time errors:

| Message | Cause and fix |
|---|---|
| `X needs module Y, which is not installed` | Install the dependency first (or fix `depends`). |
| `X is a built-in module` | Your id clashes with `base`, `product`, `crm`, `sales`, `invoicing` or `platform`. |
| `X 0.1.0 is installed; installing 0.1.0 is not an upgrade` | Raise `version`. |
| `model X already exists` | Another module owns a model of that name. |
| `M.F refers to model T, which is not installed` | A `ref` into another package or built-in: install it and add it to `depends`. |
| `another installed module depends on X` | On uninstall: remove the dependents first. |

## 4. Field kinds and their JSON

Values travel as JSON on REST and MCP, and as `key=value` text on the CLI (which converts `number`, `money` and `bool` using the metadata). The server validates every create and update (HTTP 400, `<model>.<field> must be <description>`); existing records are not re-validated.

| Kind | JSON | Notes and example |
|---|---|---|
| `text` | string | One line. `"MBP-0042"` |
| `longtext` | string | Multi-line (the UI shows a text area). `"line 1\nline 2"` |
| `number` | integer | No floats. Use it for counts, minutes, basis points. `1020` |
| `money` | integer | Minor currency units, never a float or a numeric string. 12.34 EUR is `1234`. The package does not store a currency unless you add a field for it. |
| `date` | string `YYYY-MM-DD` | Must be a real calendar date (leap years honoured). `"2026-01-31"` |
| `bool` | `true`/`false` | |
| `select` | string | One of `options`. `"in_stock"` |
| `ref` | string | The id of an existing record of model `to`; a dangling id is refused. `"9f3c..."` |

Conventions the bundled packages follow: refs are named `<thing>_id`; time of day is a `number` of minutes since midnight (540 is 09:00) next to a `date`; rates are `number`s in basis points (2000 is 20.00 percent); free-text codes such as invoice numbers are `text` because packages cannot generate sequences; yes/no with more than two states is a `select`.

## 5. Data classes, personal data and permission groups

### Which class

A data class tags a field for masking, export and retention (CONVENTIONS section 5, D40, D41). Pick the strictest that fits; a value from which a protected value can be recomputed carries the strictest class of its inputs.

| Class | Use for | Examples |
|---|---|---|
| `none` | Not personal and not otherwise sensitive. Must be written explicitly. | Product code, state, quantity, own price |
| `public_directory` | Directory data every internal user may see. | Employee name, work email, job title, blog author |
| `internal` | Business data about people and work. The default choice for a person's name, email or address in operations. | Ticket requester email, holder, technician, signer name, site address |
| `confidential_hr` | Personal employment facts that are not money. | Contract type, home address, national id, birth date |
| `special_category` | GDPR Art. 9: health, union, work permit, biometrics. | Sick-leave flag |
| `financial` | Money owed to, paid to or held for a named person or counterparty; bank accounts; payment references. | Salary, IBAN, expense amount, bank statement lines |
| `secret` | Credentials and keys; write-only. | API keys, webhook secrets |

`confidential_hr`, `special_category` and `financial` are the sensitive classes: they are withheld from agents and non-local AI routes by default and get stricter handling. A class is not a permission group; use `group` on the field to restrict who sees it.

`personal_data = true` marks a field as about an identifiable person; it drives access and erasure requests (D33). A personal-data field may never be `none` (the gate refuses it). A field can be sensitive without being personal data (a company's bank balance is `financial`, not personal).

### Groups

A group is just a name `module.role`; it exists as soon as something refers to it. Conventions: `<id>.user` (day-to-day), `<id>.manager` (configuration and approvals); add a third group (`approver`, `finance`, `special_reader`) only when a separation of duty is real: someone who enters a thing must not be the person who releases it. The administrator group `platform.admin` satisfies every check for a human; an agent token only has the concrete groups listed in its scopes, and can only narrow its owner's groups.

Three places take a group:

1. Model `read_group` / `write_group`: read (`get`, `list`) and write (`create`, `update`).
2. Field `group` (default: the model's read group): who sees the field. A field the principal cannot read is not described to it in `/api/meta`.
3. Action `group` (default: the model's write group): who may run the step. This is how you require a stricter group for posting, paying, closing.

## 6. Workflow actions and approvals

An action is the only custom behaviour X1 offers: a guarded assignment `field := to` valid when `field == from`. Chain actions on one `state` select to build a workflow; you may declare several actions with different `from` values (for example `refund` from `paid` and `refund_delivered` from `delivered`), and several actions can lead to the same state. Action names must be unique per model, so use distinct names for the two refunds.

What the dispatcher does with `approval = true`:

- A **human** who holds the action's group runs it directly and it takes effect at once.
- An **agent** (a principal made by `agent-token`, or an MCP client) that calls it gets `ApprovalRequired { plan_id }`; nothing changes. A human reviews and approves the plan (`POST /api/approve/{plan_id}`, or the approval prompt in the UI), and only then the action runs, as the human.
- Agents may run actions without `approval` directly, within their scopes.

Mark `approval = true` on anything irreversible or money-moving: posting to a ledger, paying, refunding, sending to customers, publishing publicly, closing a period, retiring an asset. Reversible bookkeeping steps (assign, schedule, handle) normally do not need it.

What actions cannot do: set more than one field, compute anything, create records, notify, call out, or check other fields (for example that required fields are filled). They are also not the only way to change a state: see section 10.

## 7. Refs, `depends`, installing, upgrading, removing

- A `ref` to a model in your own package is verified when the file is parsed. A ref to any other model (built-in `party`, `product`, `sale_order`, `invoice`; or another package's) needs that model to exist at install time and the module that owns it in `depends`. Built-in module ids to use: `base` (party), `product`, `sales` (sale_order), `invoicing` (invoice), `crm`. Packages: their own id (`hr` for `hr_employee`).
- **Install** validates the file, runs the gate, checks `depends`, refuses name collisions, registers the models, and stores the manifest. Order matters only through `depends`: install dependencies first.
- **Upgrade**: install a file with the same `id` and a **higher** `version` (compared as three numbers). The module is unregistered and registered again from the new file. The stored records are not touched. There are **no migrations**: if you rename or remove a field or model, old records keep their old data in the store but the new definition no longer exposes it; if you add a `required` field, old records are not re-validated. Prefer adding optional fields.
- **Uninstall** (`obop modules uninstall ID`, Customize, or `module.uninstall`; an administrator only) removes the commands and the models from the app; the **data stays** in the database, and installing the module again brings every record back. A module cannot be uninstalled while another installed package lists it in `depends`.
- Install, upgrade and uninstall are `platform.admin` commands that agents may only propose.

## 8. Testing and publishing

### Test a package

- `cargo run -- modules install FILE --db sqlite:///path/x.db` runs exactly the checks of an upload: parse, validation, the D41 gate, dependencies, refs. It prints the models and groups it created, or the error. Use a throwaway database; install dependencies first with `modules install-from ID` (for bundled packages).
- `cargo run -- modules check` runs the gate over the *linked* (built-in) modules only; it does not read your file.
- Customize app, **Module builder**: define models, fields (name, description, kind, options, target), and actions in a form; it writes the `obop.toml` for you and installs it. Customize, **Upload your own** accepts any `.toml` file and shows the error text on failure.
- To exercise a package like the bundled ones, copy the pattern of the `bundled_packages` tests in `crates/obop/src/main.rs`: install through the real installer, create a record per model from the metadata, run every action in and out of order, check agents are only given a proposal for approval actions.
- Try as an ordinary user: `echo "a-long-password" | cargo run -- admin create alice --groups assets.user --db sqlite:///path/x.db`, then log in and see which fields and buttons appear.

### Publish

The marketplace is a static host. Serve over HTTP or HTTPS:

```
index.json                 [{"id":"assets","name":"Assets","version":"0.1.0","description":"...","author":"Example Ltd"}, ...]
packages/assets.toml       the obop.toml of each package listed
```

`index.json` is an array of objects with `id`, `name`, `version`, `description` and optional `author`. Package ids may contain only lowercase letters, digits and `_` (anything else is refused before a request is made). Point an instance at it with the environment variable `OBOP_MARKETPLACE_URL=https://host/path` (no trailing slash needed). The instance shows the bundled catalogue first and your host after it; if the same id exists in both, the bundled one wins. Every download is validated exactly like an upload, and responses are limited to 1 MiB. The bundled catalogue itself is `packages/*.toml`, compiled into the binary by `crates/kernel/obop-pkg/src/marketplace.rs`. Package signing and paid listings are not built (D45).

## 9. Packages as data: the dashboards widget schema

The `dashboards` package stores dashboards as records so a UI can render them later; it never runs a query itself and does not check that the model or fields named in a widget exist.

`dashboards_dashboard`: `title` (required), `owner` (required, name; personal data), `description`, `shared` (bool), `position` (number; lower first).

`dashboards_widget`:

| Field | Kind | Meaning |
|---|---|---|
| `dashboard_id` | ref `dashboards_dashboard`, required | The dashboard the tile sits on. |
| `title` | text, required | Tile title. |
| `kind` | select, required: `kpi`, `bar`, `line`, `pie`, `table` | How the tile is drawn. |
| `model_name` | text, required | Model to read, as in `/api/meta`, for example `sale_order`. |
| `measure_field` | text | Number or money field to aggregate; empty when counting. |
| `aggregate` | select, required: `count`, `sum` | How records are combined. `sum` needs `measure_field`. |
| `group_by` | text | Field to split by (a select, ref or date field), for `bar`, `line`, `pie` and `table`; empty for a single `kpi` value. |
| `filter` | longtext | A JSON object of field name to exact value, for example `{"state":"paid"}`; empty means all records. Only equality is defined. |
| `position` | number | Order of the tile on the dashboard; lower first. |

## 10. Limitations

Declarative X1 deliberately cannot do the following. Plan around them or wait for X2 and X3.

- **No computed fields.** Totals, balances, ages, taxes and margins are entered by whoever creates the record. The accounting package does not check that debits equal credits.
- **No sequences or unique constraints.** Numbers such as `INV/2026/0001` are free text; uniqueness and gaplessness are conventions the people entering them keep. Two records can have the same `tag` or `slug`.
- **No row-level permissions.** Access is by model and field group only. Everyone holding `knowledge.user` reads every article regardless of its `visibility` field; that field is a label.
- **No side effects.** Actions change one field. They do not send email, create invoices, post to a ledger, move stock or call a payment provider. The email, sign, eCommerce and POS packages record what happened or should happen; something else must do it.
- **State is not locked.** The generic `update` command can set any field, including `state`, to any valid option for a person who holds the model's `write_group`; the stricter group on an action applies only to the action. Signed or posted records are "immutable by convention" only. If a transition must be restricted, make the model's `write_group` the stricter group and keep the day-to-day work in a separate model, or wait for X2 validation rules. A ref and `required` are also only checked on create and update, not when an action runs.
- **No delete command**, no cascade, no cross-record validation (dates in order, totals matching lines).
- **No migrations**: see section 7.
- **Lists are plain**: `list` returns all records; filtering, sorting, grouping and search happen in the client.
- Limits: 256 KiB, 50 models, 200 fields per package; no floats; one workflow field per action.

## 11. FAQ

**Why must I write `class = "none"`?** So every field has been looked at. The D41 gate refuses a field with no class; "not personal" is a statement you make on purpose.

**My field holds a person's name; which class?** `public_directory` if every employee may see it (an author or a department head), otherwise `internal`, plus `personal_data = true`. Pay or bank data: `financial`. Health, union: `special_category`.

**Why does an agent get "approval required"?** The action has `approval = true`. That is by design: agents draft, humans commit (D18).

**Can I make two different transitions to the same state?** Yes, with two actions with different names and `from` values.

**Can an action have several `from` states?** No; declare one action per `from`. Leave `from` out only if the step is valid from anywhere.

**Can a ref be optional?** Yes; only `required = true` forces it. A self-ref cannot be required.

**How do I store a currency?** Add a `text` field (`currency = "EUR"`) next to the `money` fields. Amounts are integers in minor units (cents).

**How do I model a time of day?** A `date` plus `number` fields of minutes since midnight, as `planning` does.

**How do I add a field to an installed module?** Add it to the file, raise `version`, and install the file again. Old records have no value for it.

**Where do I put a model that belongs to two apps?** Models are named after the module that owns them. Add the module that owns the model to `depends` and `ref` to it.

**Can a module have no models?** The file parses with no `[[model]]`, but it adds nothing.

**Where are decisions recorded?** D11 (tiers), D45 (packages and the admin), D46 (backoffice and Customize), D47 (typed fields) in `docs/DECISIONS.md`; data classes in `docs/CONVENTIONS.md` section 5.
