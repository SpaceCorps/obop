# How to write a module

Audience: a developer or a business analyst who wants to add an app to obop without writing Rust. Everything here is checked against the code (`crates/kernel/obop-pkg/src/manifest.rs`, `installer.rs`, `marketplace.rs` and the command layer in `obop-commands`). The tutorial module is a real file, `docs/modules/assets.toml`, and the end-to-end test `the_tutorial_module_and_its_documented_errors_are_real` in `crates/obop/src/main.rs` installs it and triggers every error quoted in section 3.5.

## 1. What a module is

A module (also called a package) is one text file, `obop.toml`. It declares **models** (record types), their **fields** (each with a kind, a data class, a description and a permission group) and **actions** (state transitions that can also assert, create, change and adjust other records). Installing it registers the models with the one command dispatcher, so the same module is immediately available as REST endpoints, MCP tools for agents, CLI commands and backoffice screens (list, kanban, form, status bar, buttons), with masking, permission checks and agent approval applied for free (D29, D41, D45, D46).

### Tiers

| Tier | What it can contain | Status |
|---|---|---|
| X1 declarative | Models, typed fields, data classes, permission groups, state-transition actions with assertions and effects, computed fields, unique fields, sequences, row-level access. No code. | Built (this document; D45, D47, D56 to D58) |
| X2 CEL | Richer expressions stored in data: modifiers, policies, date arithmetic, validation across records. The small expression language of section 3.8 is its first part. | Partly built (D56, D57) |
| X3 Wasm | Sandboxed components for logic that expressions cannot express (connectors, side effects). | Not built (D11) |

X1 packages cannot run anything, so an administrator can install one from an upload at runtime. When X2 and X3 arrive a package will gain sections for expressions and components; the X1 sections described here stay valid. Behaviour that needs code (calling out, notifying, payments, anything outside the database) waits for those tiers.

### What ships

Built-in modules (Rust crates, not removable): `base` (model `party`), `product` (`product`), `crm`, `sales` (`sale_order`), `invoicing` (`invoice`), `connectors` (`connection`, `consent`, the provider catalogue and the other records of docs/CONNECTORS.md). Bundled packages in `packages/*.toml` (installed from the Customize app or `obop modules install-from ID`). Built-in modules version with the core (`0.2.0`, D55); every bundled package declares `license = "AGPL-3.0-or-later"`, `requires_core = "^0.2"`, its own version and its dependencies with ranges:

| Package | Needs | What it holds |
|---|---|---|
| `accounting` | base | Chart of accounts, journals, taxes (basis points), fiscal years and periods, journal entries and lines, bank statement lines. Posting and closing need approval and `accounting.approver` |
| `appointments`, `documents`, `expenses`, `fleet`, `helpdesk`, `manufacturing`, `projects`, `purchase`, `inventory`, `subscriptions`, `timesheets`, `hr` | varies | Services, supply chain and people apps |
| `planning` | hr | Roles, shift templates, shifts (open, published, confirmed) |
| `field_service` | base, product, hr | Jobs with site, technician, parts used, signature flag |
| `pos` | base, product | Point-of-sale config, cash-controlled sessions, orders, lines, payments; refunds and session close need approval |
| `ecommerce` | base, product | Categories, listings and translations, shipping methods, discount codes, web orders and lines; public shop and checkout when the site is enabled; refunds need approval |
| `website` | none | Pages, menu items, blog posts, form submissions and translations; served publicly when the site is enabled (docs/STOREFRONT.md) |
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
summary = "A register of company equipment with a holder and a life cycle from stock to retired."
description = "A register of company equipment: categories, individual assets with a holder, and a stock, assigned, in repair, retired life cycle. Purchase prices are visible to managers only; retiring an asset needs a human approval."
author = "Example Ltd"
license = "Apache-2.0"
homepage = "https://example.com/assets"
keywords = ["assets", "equipment"]
categories = ["productivity"]
maturity = "beta"
requires_core = "^0.2"

[module.dependencies]
base = "^0.2"
```

`id` names the module and prefixes everything it owns: every model must be called `assets` or `assets_<something>`, and by convention permission groups are `assets.<role>`. `version` is a semantic version (`0.1.0`, or `0.2.0-beta.1`); a later install of the same id must have a higher version. `name` is what the app launcher shows. `description` is shown in the module panel and is required; `summary` is the one line (at most 140 characters) on the module's card. `license` is an SPDX expression and `requires_core` says which core versions the module runs on (section 3.1). `[module.dependencies]` lists the modules that must be installed, each with the versions it accepts; because `assets_asset` has a ref to `party` (a built-in model of `base`), we depend on `base`. Built-in modules version with the core, so `base = "^0.2"` is checked against the core version.

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

A model needs a description and two groups: people holding `read_group` can read records (`get`, `list`, `search`); people holding `write_group` can `create`, `update` and `delete`. Every field needs a description and a data class. `class = "none"` is how you say "this is not personal data and not otherwise sensitive"; it must be written out. `required = true` makes the server refuse a create without the field. Fields default to `kind = "text"`.

Each model automatically gets seven commands, with the same names on REST, MCP and the CLI: `<model>.get`, `.list`, `.search`, `.create`, `.bulk_create`, `.update` and `.delete`. Records get an `id` assigned by the server. `list` and `search` take the optional `filter`, `order`, `limit` and `offset` of section 4.1; `delete` removes a record that no other record refers to and, being destructive, is only *proposed* by an agent. A field that is the target of an action is governed (section 6.1): `create` and `update` cannot set it freely.

`bulk_create` (D70) makes up to 1,000 records in one call, from `rows` (a list of objects) or `csv` (the text of a CSV file whose header names the fields). Each row gets exactly the checks of `create` (kinds, required fields, references the caller may see, governed fields starting at their first option, unique values also among the rows of the call, sequences, `own` defaulting to the caller, the row-level rule that a creator may not make what they could not then see; computed and numbered fields cannot be set), the write group of the model is the permission, and an agent needs approval exactly where `create` would. The call is all or nothing unless `mode` is `partial`; `dry_run` checks without writing; `idempotency_key` makes a retried call return the first answer. A CSV file is UTF-8 (a byte order mark is dropped), separated by commas or semicolons, at most 1 MiB; references are the **id** of the record they point to (there is no way to point at a record of the same file, and no remapping of ids); numbers and money are whole numbers (money in minor units such as cents: a decimal point is an error, because there are no decimal kinds yet); yes/no fields take `true`, `false`, `yes`, `no`, `1` or `0`; dates are `YYYY-MM-DD` (there is no date-time kind yet); an empty cell leaves the field out; **image and file fields cannot be imported** (upload and attach them one record at a time). A module needs to do nothing to get it, and a model that its caller cannot `create` does not offer it.

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

- `category_id` refs a model of the same package (`assets_category`); it is checked when the file is parsed. `supplier_id` refs `party`, which belongs to `base`: that is why `base` is in `[module.dependencies]`, and the installer also checks at install time that `party` is registered. `replaces_id` points at its own model, which is allowed (a ref to itself is optional here; it could not be `required`, because the first record could never be created).
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

Installing it again fails with `assets 0.1.0 is already installed`. Edit `version` to `0.2.0` and it upgrades in place. In the backoffice open Customize, Modules, Marketplace, "Upload your own package" and pick the file: the page checks it and shows what installing it would do before anything happens. Then find "Assets" in the app launcher. Note that `platform.admin` holds every group, so an admin sees all of it; create a user with `--groups assets.user` to see what an ordinary employee sees.

## 3. Manifest reference

The file is TOML, at most **256 KiB**, with at most **50 models** and **200 fields in total** across all models. Unknown keys anywhere are errors (`invalid obop.toml: unknown field ...`). Identifiers (`id`, model, field and action names, and both halves of a group name) are lowercase ASCII letters, digits and `_`, start with a letter, at most 40 characters.

### 3.1 `[module]` (one, required)

| Key | Type | Required | Rules |
|---|---|---|---|
| `id` | string | yes | Identifier. Must not equal a built-in module id. |
| `version` | string | yes | A semantic version: `major.minor.patch`, optionally with a pre-release (`1.0.0-rc.1`). Numbers have no leading zeros. |
| `name` | string | yes | Not blank. |
| `description` | string | yes | Not blank. |
| `summary` | string | no | One line, at most 140 characters: the text of the module's card. Without one the card shows the first sentence of the description. |
| `author` | string | no | At most 100 characters. Shown in listings. |
| `authors` | array of strings | no | At most 10 names. Shown with `author`. |
| `license` | string | **catalogue: yes** | An SPDX expression (`Apache-2.0`, `AGPL-3.0-or-later`, `MIT OR Apache-2.0`, `GPL-3.0-only WITH Classpath-exception-2.0`), `LicenseRef-<name>` for your own terms, or `proprietary` / `UNLICENSED`, which are allowed and flagged on the card. Required for packages in the bundled catalogue and the marketplace; optional (with a warning) for an upload. |
| `homepage`, `repository`, `documentation`, `support` | string | no | `https://` URLs only, at most 200 characters, no spaces. |
| `keywords` | array of strings | no | At most 10; each 2 to 24 characters of lowercase letters, digits and `-`; unique. Searchable. |
| `categories` | array of strings | no | At most 3 of `finance`, `sales`, `supply-chain`, `people`, `services`, `marketing`, `productivity`, `platform`, `other`. |
| `icon` | string | no | The name of a built-in icon (lowercase letters, digits, `-`). An unknown name falls back to a generic icon; by default the icon follows the module id. |
| `maturity` | string | no | `stable`, `beta` or `experimental`. Nothing is shown when absent. |
| `requires_core` | string | **yes (D63)** | The core versions the module runs on, a semver range such as `"^0.2"` or `">=0.2, <0.4"`. Required for every package, upload or not: missing, empty and unsatisfiable ranges are refused. A range with no upper bound (`"*"`, `">=0.2"`) is a warning for an upload and an error for the bundled catalogue and the marketplace. An install on a core that does not satisfy it is refused. |
| `changelog` | string | no (D63) | The release notes of this version, plain text, at most 8000 characters. Shown before an update, in the plan dialog and the Versions tab. |
| `depends` | array | no | **Retired (D63).** The any-version shorthand is refused with a message that says what to write instead: every dependency names the versions it supports in `[module.dependencies]`. |
| `network_destinations` | array | no | Must be empty or absent: declarative packages may not reach the network. The key exists so later tiers share the contract. |

Since D63 `requires_core` and a range for every dependency are required; the other keys of D55 stay optional. Unknown keys are still errors.

#### `[module.dependencies]`: other modules, with versions

```toml
[module.dependencies]
base = "^0.2"           # a built-in module: checked against the CORE version
projects = ">=0.3, <1"  # a package: checked against its installed version
```

The table maps a module id to the **range of versions of it that this module supports**, a string that is required for every dependency (an empty string, a range nothing satisfies such as `">=2, <1"`, and the retired `depends = [...]` shorthand are errors that name the dependency). A wildcard `"*"` parses but promises to work with every future version, so it is a warning for an upload and an error for the catalogue (`obop modules check --catalog`). The range is enforced both ways: installing or updating the module checks it against what is installed, and updating, rolling back or uninstalling a module checks every installed dependent's declared range against the version that would result. The plan dialog shows each dependent with its range and whether it would break.

 The shape is a table of strings on purpose (no inline tables, no flags): optional dependencies are not offered, because a `ref` to a model whose module is not installed cannot work, so a dependency that may be absent would be a promise the registry cannot keep. A module may not depend on itself and may declare at most 20 dependencies.

**Requirement syntax** (the rules of Cargo, so a Rust developer already knows them): `*` any release; `1.2.3` or `^1.2.3` compatible with 1.2.3 (below 2.0.0; for `0.y.z` the minor is the breaking part, so `^0.2` is `>=0.2.0, <0.3.0`); `~1.2.3` patch updates only (`>=1.2.3, <1.3.0`); `>=1.2`, `>1.2`, `<=1.2`, `<1.2`, `=1.2.3`; partial versions and wildcards (`1.*`, `=1.2`); comma-separated comparators are ANDed (`>=0.2, <0.4`). A pre-release such as `0.3.0-beta.1` satisfies a requirement only when that requirement names a pre-release of the same `major.minor.patch`; otherwise only releases match.

**Built-in modules version with the core.** `base`, `product`, `crm`, `sales`, `invoicing`, `connectors` and `platform` have the version of the running core (`GET /api/meta` says which). A requirement on one of them is therefore a requirement on the core: `base = "^0.2"` holds on core 0.2.x and fails on 0.3.0.

#### Compatibility policy

The core follows semantic versioning. While it is below 1.0, a **minor** bump (`0.2` to `0.3`) may break manifests, commands or the API and a **patch** bump never does; from 1.0 only a major bump may. A package therefore writes `requires_core = "^0.2"` (runs on 0.2.x), widens it to `">=0.2, <0.4"` after testing on 0.3, and bumps its own version by the same rule. A package that depends on another package picks a range it has tested, for example `projects = ">=0.3, <1"`, so a breaking release of `projects` (a new major version, or a new minor while it is below 1.0) is caught by the resolver before anything is installed.

### 3.2 `[[model]]` (any number)

| Key | Type | Required | Rules |
|---|---|---|---|
| `name` | string | yes | Identifier equal to the module id or starting with `<id>_`; unique in the file; must not already exist in the system. |
| `description` | string | yes | Shown in the gate and the UI. |
| `read_group` | string | yes | `module.role`. Needed for `get`, `list`. |
| `write_group` | string | yes | `module.role`. Needed for `create`, `update`. |
| `field` | array of tables | no | See 3.3. |
| `action` | array of tables | no | See 3.4. |
| `access` | table | no | Row-level access, see 3.9. |

`[[model.field]]` and `[[model.action]]` are written after their model, indented by convention.

### 3.3 `[[model.field]]`

| Key | Type | Default | Rules |
|---|---|---|---|
| `name` | string | required | Identifier, unique in the model, never `id`. |
| `description` | string | required | Not blank (the gate refuses `field has no description`). |
| `class` | string | required | One of `none`, `public_directory`, `internal`, `confidential_hr`, `special_category`, `financial`, `secret`. See section 5. |
| `personal_data` | bool | `false` | `true` if the value is about an identifiable person. Not allowed with `class = "none"`. |
| `group` | string | the model's `read_group` | `module.role`. Principals without it do not see the field. |
| `kind` | string | `text` | `text`, `longtext`, `number`, `money`, `date`, `datetime`, `decimal`, `bool`, `select`, `ref`, `sequence`, `image`, `file`. |
| `precision`, `scale` | integer | none | Only for `decimal`, and then both are required: `precision` is the number of digits in all (1 to 38), `scale` the digits after the point (0 to 12, not above the precision). `precision = 12, scale = 2` holds `-9999999999.99` to `9999999999.99`. |
| `round` | string | none | Only for `decimal`: `half_even`, `half_up` or `down`. On a stored field it is how an import (`bulk_create`, `obop import`) cuts input that has too many fractional digits; without it such input is an error everywhere. On a computed field it is the rounding of every decimal division and of the result (default `half_even`). |
| `max_bytes` | integer | 5 MiB (`image`), 10 MiB (`file`) | Only for `image` and `file`: the largest upload accepted for the field. |
| `options` | array of strings | none | Only for `select` (then required, non-empty, no blanks, no duplicates). |
| `to` | string | none | Only for `ref` (then required). Model name. |
| `required` | bool | `false` | Must be present and non-null on create; cannot be set to null on update. |
| `computed` | string | none | An expression (3.8). The field is computed when the record is read and is read-only; `text`, `number`, `money`, `bool`, `decimal` or `datetime` only; not `required` or `unique`. |
| `unique` | bool | `false` | No two records of the model may have the same non-empty value. |
| `unique_with` | array of field names | `[]` | With `unique`: the value need only be unique among records equal in these fields (at most 4). |
| `format`, `assign_on`, `gapless`, `reset` | | | Only for `kind = "sequence"`, see below. |

`kind = "sequence"` is a text field numbered by the system (read-only, unique, never `required`): `format` is required, for example `"INV-{year}-{seq:05}"` (letters, digits and `- _ / . # space`, exactly one `{seq}` or `{seq:N}` with N from 1 to 12 for zero padding, optional `{year}` and `{month}`); `assign_on` names the action that numbers the record (default: numbered when it is created); `gapless = true` forbids deleting a record that holds a number; `reset` is `never` (default), `year` or `month` and needs `{year}` (and `{month}`) in the format. The counter is kept in a `sequence_counter` record per model, field and period and is committed in the same transaction as the record, so a failed action uses no number.

### 3.4 `[[model.action]]`

| Key | Type | Default | Rules |
|---|---|---|---|
| `name` | string | required | Identifier; not `get`, `list`, `search`, `create`, `bulk_create`, `update` or `delete`; unique within the model (a duplicate is refused). The command id is `<model>.<name>`. |
| `description` | string | required | Not blank. |
| `field` | string | required | A `select` field declared in the same model; any select field, not only `state` (the UI draws `state` as the status bar and kanban, other selects are governed the same way). That field becomes *governed* (section 6.1). |
| `from` | string | none | The value the field must have now; one of the options. Without `from` the action works from any value; the bundled packages always set it. Must differ from `to`. |
| `to` | string | required | The value to set; one of the options. |
| `group` | string | the model's `write_group` | `module.role`. |
| `approval` | bool | `false` | Agents may only propose it (D18). |
| `assert` | array | `[]` | Conditions that must hold before the action runs, each an expression (3.8) as a string, or `{ expr = "...", message = "..." }`. At most 8. |
| `effect` | array of tables | `[]` | What the action does besides moving the state, `[[model.action.effect]]` (3.8). At most 12. |

### 3.5 Validation errors you can hit

Parse and gate errors (from `obop modules install`, the Customize upload or `module.install`), and how to fix them:

| Message contains | Cause and fix |
|---|---|
| `invalid obop.toml: ...` | Not valid TOML, a missing required key, a value of the wrong type (for example `required = "yes"`), or an unknown key. Read the line and column in the message. |
| `manifest is larger than 256 KiB` | Split the module. |
| `module id "X" must be lowercase letters, digits and _, starting with a letter` | Fix the id. |
| `version must look like 1.2.3` | Three dot-separated numbers without leading zeros; a pre-release may follow (`1.2.3-beta.1`). |
| `summary must be one line of at most 140 characters` | Shorten it; no line breaks. |
| `author is longer than 100 characters` / `authors takes at most 10 names of 1 to 100 characters` | Shorten the names or list fewer. |
| `license: "X" is not a known SPDX licence identifier (...)` / `license: ...` | Use an SPDX identifier or expression (`Apache-2.0`, `MIT OR Apache-2.0`), `LicenseRef-<name>` for your own terms, `proprietary` or `UNLICENSED`. The check is case-insensitive for identifiers but `AND`, `OR` and `WITH` are upper case. |
| `homepage must be an https:// URL (got "...")` (also `repository`, `documentation`, `support`) | Use https; other schemes and plain http are refused. |
| `keyword "X" must be unique, 2 to 24 lowercase letters, digits or -` / `keywords takes at most 10 entries` | Fix or drop the keyword. |
| `unknown category "X" (use finance, sales, ...)` / `categories takes at most 3 entries` / `category "X" is listed twice` | Pick from the fixed list. |
| `icon "X" must be a lowercase icon name such as "project"` | Letters, digits and `-`, starting with a letter. |
| `maturity must be one of stable, beta, experimental (got "X")` | Use one of the three. |
| `requires_core: "X" is not a version requirement (...); examples: "*", "^0.2", ">=0.2, <0.4"` | Fix the requirement; separate comparators with commas. |
| `requires_core is required: ...` | Add the core versions the module runs on, for example `requires_core = "^0.2"`. |
| `[module.dependencies] X: the version range is empty` / `matches no version` | Write the range of X you support, for example `X = ">=0.1, <1"`. |
| `depends = [...] is no longer accepted` | Move each entry to `[module.dependencies]` with a range. |
| `X depends on itself` | Remove the entry. |
| `[module.dependencies]: "X" is not a module id` / `[module.dependencies] X: ...` | A requirement is a string such as `">=0.2, <1"`; an inline table (`{ version = ..., optional = true }`) is a parse error. |
| `a module may declare at most 20 dependencies` | Split the module. |
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
| `M: bad action name "X"` | Not an identifier, or one of `get`, `list`, `search`, `create`, `bulk_create`, `update`, `delete`. |
| `M: action "X" is declared twice` | Two actions of one model share a name; use distinct names (two ways to one state need two names). |
| `M.A: acts on undeclared field "X"` | `field` must name a field of the same model. |
| `M.A: acts on F, which is a text field; an action moves a select field between its options` | Make the field `kind = "select"` with `options`. |
| `M.A: "V" is not one of the options of F (...)` | `from` or `to` is not in the select's options. |
| `M.A: `from` and `to` are both "V"; an action must change the state` | An action that changes nothing is refused. |
| `M.F: computed: ...` / `computed `E` is a number, but the field is text` | The expression does not parse or its type is not the field's. |
| `computed `E` reads M.F (class C); the field must have class ...` / `give the computed field `group = "g"`` | A value computed from protected data is protected too: same group, same or stricter class, personal data if an input is. |
| `computed fields depend on each other in a cycle: a -> b -> a` | Remove the loop. |
| `M.F: format: ...` / `reset = "year" needs {year} in the format` / `assign_on: X is not an action of the model` | Fix the sequence. |
| `M.F: unique_with: no field X` / `a computed field cannot be unique` | Fix the constraint. |
| `M.A: assert "E": ...` / `assertion `E` is a number; it must be a comparison` | Assertions are true/false expressions over fields of the model and its children. |
| `M.A effect N: ...` (`has no field`, `is required, so the effect must set it`, `must be an integer; that value does not fit`, `is a governed state field; use a set_state effect`, `for_each: ...`, `no earlier create effect is named X`, `has no action X`) | See 3.8. |
| `effects form a cycle: a.x -> b.y -> a.x` / `more than 4 deep` | `set_state` effects may not loop. |
| `X refers to model T, which no dependency of P provides` (for an effect on T) | Add the module that declares T to the dependencies. |
| `access.own: no field F` / `must be a stored text or ref field` / `access.via: F is a cycle` / `[model.access] declares no rule` | See 3.9. |
| `...: personal data cannot be classed `none`` | The D41 gate: choose a real class for a field with `personal_data = true`. |
| `...: field has no description` / `command has no description` / `model has no description` | Descriptions are mandatory (they are what agents and the UI show). |

Install-time errors:

| Message | Cause and fix |
|---|---|
| `X needs Y R, which is not installed and not offered by the marketplace` | Nothing provides `Y`: fix the id, or install a package that is. Dependencies the marketplace offers are installed for you. |
| `X needs Y R, but the marketplace offers Y V` | The marketplace's version of `Y` does not satisfy `R`: loosen `R` or publish a fitting `Y`. |
| `X V needs core R, you run C` | The package says it needs another core than the one running; nothing is installed. Use a package version for this core or upgrade the instance. |
| `X needs Y R, a built-in module that versions with the core; you run core C` | Built-ins version with the core, so the requirement is a core requirement. |
| `dependency cycle: a -> b -> a` | Two packages need each other: remove one edge. |
| `Y V is installed but X needs ...` / `... which does not satisfy: ...` | An installed dependency is too old (or new) and the marketplace has nothing that fits. |
| `upgrading Y to V would break X, which needs Y R` | An installed module needs a `Y` that `V` is not. Upgrade `X` first or choose another version. |
| `X V is already installed` / `... would be a downgrade, which is refused` | Raise `version`; an installed module is never moved to an older or the same version. |
| `X is a built-in module` / `X is a built-in module and is part of the core` | Your id clashes with `base`, `product`, `crm`, `sales`, `invoicing`, `connectors` or `platform`. |
| `model X already exists` | Another module owns a model of that name. |
| `M.F refers to model T, which no dependency of X provides` | A `ref` into another package or built-in: add the module that declares `T` to the dependencies. |
| `marketplace package X: license is required for marketplace and bundled packages ...` | A catalogue package must declare `license` and `requires_core`. |
| `X: ...; the N step(s) before it were rolled back and nothing was installed` | A step of a multi-module install failed; the earlier steps were undone. |
| `required dependencies cannot be skipped ...` | `without_dependencies` may only be false. |
| `X is required by Y (and, through them, Z); uninstall those first` | On uninstall: remove the dependents first. |

### 3.6 `[model.public]`: what visitors may see and send (D50)

Optional, one per model, written right after the `[[model]]` keys. Without it nothing of the model is public. Needs the instance to run with `--public-site` (docs/STOREFRONT.md); the declaration itself is checked on every install.

| Key | Type | Rules |
|---|---|---|
| `read` | bool | Anonymous visitors may read records. Needs `published_field` and `fields`. |
| `published_field` | string | A `bool` field (public when true) or a `select` (public when it equals `published_value`). Records that are not public do not exist for visitors. |
| `published_value` | string | Only with a `select` `published_field`: the public option, for example `published`. Using the workflow state keeps the approval on `publish` in charge of what is public. |
| `gone_value` | string | Optional, a `select` option meaning "was public, retired" (for example `archived`): answered with 410 instead of 404. |
| `fields` | array | The only fields a public read returns. Each must be class `none` or `public_directory`, not personal data, and not restricted to a stricter `group`. A `ref` is shown as the public fields of the target only if the target model is public-read and that record is published. |
| `order_by`, `order_dir` | string | Sort of public lists; `asc` (default) or `desc`. |
| `submit` | bool | Visitors may create records. Needs `submit_fields` and `rate_limit_per_hour`. |
| `submit_fields` | array | The only fields a visitor may set. Class may be `none`, `public_directory` or `internal` (an order email is fine); never a governed state field. They are never readable publicly unless also in `fields`. `required` is still enforced. A `ref` must point at a published record of a public-read model. |
| `rate_limit_per_hour` | number | Submissions per source and hour (50 times that across all sources). |
| `receipt_token_field` | string | A `text` field of class `secret` holding an unguessable token; whoever holds it may read that one record back through `receipt_fields` (an order confirmation). Nobody can read the token itself. |
| `receipt_parent` | string | On a child model: a `ref` to the model that has a token; the record is shown with its parent (order lines). |
| `receipt_fields` | array | What the receipt shows; may include personal data the submitter gave, never `confidential_hr`, `special_category`, `financial` or `secret`. |

Errors: `M.F: only fields of class none or public_directory can be public`, `M.F: personal data can never be public`, `M.public: read = true needs published_field, so only published records are public`, `M.F: a governed state field cannot be submitted; it starts at its first option`, `M.F: a public form cannot collect class C data`.

### 3.7 `[labels.<lang>...]`: translations (D52)

English is the manifest itself. Every other language is an optional set of `[labels.<lang>]` tables in the same document, so one text installs, upgrades, is stored and served as one unit. A package without them works exactly as before.

```toml
[labels.de.module]
name = "Fuhrpark"                        # app name (launcher, sidebar, palette)
description = "Fahrzeuge, Fahrer und Wartungshistorie."

[labels.de.model.fleet_vehicle]
name = "Fahrzeug"                        # the record type, singular, nominative
description = "Ein Fahrzeug des Unternehmens"

[labels.de.model.fleet_vehicle.field.plate]
label = "Kennzeichen"                    # column header, form label, filter and group-by name
description = "Amtliches Kennzeichen"    # tooltip

[labels.de.model.fleet_vehicle.field.fuel.options]   # select options: value = label
petrol = "Benzin"
electric = "Elektro"

[labels.de.model.fleet_vehicle.action.dispatch]
label = "In Betrieb nehmen"              # button
description = "Ein verfügbares Fahrzeug in Betrieb nehmen"
```

| Table | Keys | Shown as |
|---|---|---|
| `labels.<lang>.module` | `name`, `description` | app name; module description in Customize |
| `labels.<lang>.model.<model>` | `name`, `description` | sidebar entry, titles, "New ..." |
| `labels.<lang>.model.<model>.field.<field>` | `label`, `description` | column and form label; tooltip |
| `labels.<lang>.model.<model>.field.<field>.options` | `<option> = "label"` | select values, badges, status bar, filters, groups |
| `labels.<lang>.model.<model>.action.<action>` | `label`, `description` | action button; tooltip |

- **Label versus description.** A label is the short name (at most 80 characters) a person scans: a column header, a button. A description is a sentence (at most 400) that explains it. English has no `label`: the interface derives it from the technical name (`odometer_km` is "Odometer km"); a `[labels.en...]` table overrides that when the derived English reads badly ("Odometer (km)").
- **Language codes** are two or three lowercase letters with an optional region: `de`, `fi`, `pt-BR`. The seven the interface ships are en, de, fr, pl, fi, sv, es. Anything else matching the pattern is stored and delivered; the client uses it when the active language is that code, or its base language.
- **Every key must name something that exists**: a model of the package, one of its fields, an option of that select field, an action. A typo is a refused install, not an ignored line (`labels.de.model.fleet_vehicle.field.platte: fleet_vehicle has no field "platte"`). An unknown table or key name is refused by the schema. Option keys are the stored values and stay English.
- **Values** are plain text: not empty, no leading or trailing spaces, one line, no control characters, no placeholders or markup (`{ } < >`), at most 80 characters (400 for descriptions). They count toward the 256 KiB limit of the whole document.
- **Partial translations are fine.** Missing entries fall back (below). The bundled packages are complete in six languages, which a test enforces.
- **Permission group names** are not translatable (they are identifiers).

**Keeping the translations in a separate file.** The format is plain TOML, so the labels can live next to the manifest and be concatenated. For `assets.toml`, `obop modules check|install assets.toml` appends `assets.labels.toml` and every `labels/<lang>/assets.toml` found beside it. To publish one document, concatenate yourself: `cat assets.toml assets.labels.toml > dist/assets.toml`. A marketplace serves the combined document at `packages/<id>.toml`; the bundled catalogue does this at build time from `packages/<id>.toml` and `packages/labels/<lang>/<id>.toml`. A labels-only file can be checked on its own: `obop modules check packages/labels/de/fleet.toml`.

**How to translate your own module.**

```sh
obop modules labels assets.toml de > assets.labels.toml   # every text, English values, ready to fill in
$EDITOR assets.labels.toml                                # replace the values, keep the table headers
obop modules check --complete assets.labels.toml          # none missing, none left in English
obop modules install assets.toml                          # installs the manifest with its labels
```

`obop modules labels TARGET LANG` accepts a package file, a bundled package id (`fleet`) or a built-in module id (`crm`). `--complete` is for authors who want every text translated; without it a partial file passes. Words that are legitimately the same in your language (SKU, Hybrid) belong in `labels/identical.json` next to the language folders.

**How the lookup and fallback work.** `GET /api/meta` carries, per module, model, field and action, `labels` (`{lang: text}`) and `descriptions`, and per select field `option_labels` (`{option: {lang: text}}`), each key present only when something is translated. The client picks `labels[lang]`, then `labels[base language]` (`de` for `de-AT`), then its own table for the built-in modules, then the humanised English name; descriptions fall back to the English `description`. The English `name` and `description` fields never change, and neither do commands, MCP tool names, the CLI, stored values or error messages. Upgrading replaces the labels with those of the new document; uninstalling removes them. An instance that installed a bundled package before it carried translations receives the bundled labels for the same version when it starts.

### 3.8 Expressions, assertions and effects (D56, D57)

**The expression language** is used by `computed`, `assert` and `when`. It has integers, exact decimals (`0.25`, `12.50`: digits, a point, digits), text in double quotes (`\"` and `\\` escapes only), `true`, `false`; the fields of the record by name; `+ - * /` on integers (money and number fields; no floats exist: `/` rounds half away from zero, so `5 / 2` is 3 and `-5 / 2` is -3, and `a * b / 10000` is how a basis-point rate is applied); `== != < <= > >=` on two values of one type; `&&`, `||`, `!`; `if(condition, then, else)`; `concat(a, b, ...)` (numbers and booleans are written out); `is_set(field)`. An empty (null) value is `0` in arithmetic, `""` as text and false as a condition. Dividing by zero or leaving 64 bits is an error (a computed field then reads as empty; an assertion fails). **Decimals** (D72): a `decimal` field and a decimal literal are exact numbers; a whole number mixed with a decimal becomes a decimal, so `qty * 0.25` and `rate > 1` work and `0.1 + 0.2 == 0.3` is true. `+`, `-` and `*` lose nothing (the scale of a sum is the larger of the two, of a product their sum; going beyond 38 digits is an error) and comparison is numeric. `/` of decimals is the one operation that must cut digits: its result has the scale of the computed field being evaluated (12 in an assertion) and is rounded once with that field's `round` (default `half_even`), so `(a / b) * c` rounds `a / b` first; and the value of a computed `decimal` field is rounded to the field's scale the same way. A computed `number` or `money` field cannot take a decimal expression (the type check says so): make it a `decimal`. `sum`, `min` and `max` over a decimal child field are exact decimals. An instant (a `datetime` field) compares with an instant, or with a text literal that is a valid RFC 3339 instant (`created_at < "2026-01-01T00:00:00Z"`); an empty one is before everything; there is no arithmetic on instants and a date never compares with an instant. Aggregates look at the child records: `sum(lines.amount)`, `min(...)`, `max(...)` (empty when there are no children) and `count(lines)`, optionally `where <condition on the child>`: `count(tasks where state == "done") * 100 / count(tasks)`. The child is named by its model, or by the part of the model name after this model's name, singular or plural (`lines` in `accounting_move` is `accounting_move_line`); it must have a `ref` to this model, and `lines[move_id]` picks the ref when there are several. At most 400 characters, 80 parts, 12 levels. Types are checked when the package is parsed and installed.

**Computed fields** (`computed = "sum(lines.amount)"`) are evaluated when a record is read, from the current children, and are in `get`, `list`, `search` and every write response. They can be filtered and sorted on, may read other computed fields, are not stored, cannot be written (sending the current value back in an update is accepted, anything else is refused: `order.total is computed and read-only`), and are marked `computed: true` in `/api/meta`. Computing on read instead of keeping a cached value costs a look at the children per read (one indexed lookup for a record, one grouped scan for a list) and cannot drift. They are as protected as their inputs (3.5). Date arithmetic does not exist; `inventory_item.quantity` stays a stored number kept by `adjust` effects.

**Assertions** are checked on the record as it is, before anything is written; the failure is the message, or `assertion failed: <expression>`. Example, on a journal entry: `assert = [{ expr = "total_debit == total_credit", message = "The entry does not balance" }]`.

**Effects** run after the new state is set, in order, in one transaction with it: either the transition and every effect happen, or nothing does, sequence numbers included. They run with the caller's permissions and never more. Common keys: `kind`, `model` (what is written), `when` (an expression on the record after the transition; false skips the effect), `for_each` (repeat per child row of this record, with `$line.field` available; at most 500 in all), `as` (a `create` effect names its record for `$created.name`). Values (in `map`, `set`, `match`, `by`) are a constant (text, whole number, true/false) or `$self.field` (this record after the transition, computed fields included), `$self.id`, `$line.field`, `$line.id`, `$created.name`, `$now` (an instant: RFC 3339 UTC text, to the microsecond into a `datetime` field), `$today` (the date on the wall clock of the platform zone, `OBOP_ZONE`), `$caller` (who ran the action); a text constant that starts with `$` is written `$$`. The kind of every value must fit its target field (a decimal into a decimal that keeps every digit, `$now` into a `datetime`, numbers into number or money, a ref into a ref to that model, a select into a select with at least its options...) and a `create` must set every required field.

| `kind` | Keys | Does |
|---|---|---|
| `create` | `map = { field = value }` | Creates a record of `model`; governed fields start at their first option, a sequence is numbered, `own` defaults to the caller. |
| `update` | `set = { field = value }`, and `ref = "field"` or `children = true` (`via = "field"` if several) | Sets fields on the record a ref field of this record names, or on every `model` record that refers to this one. |
| `adjust` | `field`, `op = "add"/"subtract"`, `by`, and `ref = "field"` or `match = { field = value }`; `create_missing`, `min` | Adds to or subtracts from a number or money field. `match` finds the one record whose fields equal the values (several is an error; none is an error unless `create_missing`, which creates it from the match values and the amount); `min` is a floor the result may not go below. |
| `set_state` | `action`, and `ref` or `children` | Runs an action of `model` on the related records. That action's group, from-state, required fields, assertions, sequences, effects and the caller's visibility of the record all apply; a chain is at most 4 deep and may not loop. |

```toml
  [[model.action]]
  name = "confirm"
  description = "Confirm the order and prepare its invoice"
  field = "state"
  from = "draft"
  to = "confirmed"

    [[model.action.effect]]
    kind = "create"
    model = "invoice"
    as = "invoice"
    map = { sale_order_id = "$self.id", party_id = "$self.party_id" }

    [[model.action.effect]]
    kind = "create"
    model = "invoice_line"
    for_each = "lines"
    map = { invoice_id = "$created.invoice", quantity = "$line.quantity", unit_price_minor = "$line.unit_price_minor" }
```

Rules the engine keeps: the caller needs the write group of every model an effect writes and the group of every action it starts, or the whole action is refused before it runs (`order.confirm also changes invoice records, which needs the invoicing.user group`); a record the caller cannot see (3.9) is not available to an effect; an agent that triggers an action reaching an `approval = true` action only proposes the whole operation, and the approver must hold all the groups; the 500-record cap, the effect count and the nesting depth are enforced with clear errors; a failure names the effect (`order.confirm: effect 3 (adjust) failed, nothing was changed: ...`). Seeds and direct store writes run no effects. Each bundled workflow is listed in section 12.

### 3.9 `[model.access]`: row-level access (D58)

Group checks decide which commands a person may call; access rules decide which *records* they see. Optional, one per model, written after the model keys.

| Key | Type | Meaning |
|---|---|---|
| `own` | string | A text (or ref) field holding the user id (login name) of the person the record belongs to. That person sees and edits it. On create it defaults to the creator. |
| `via` | string | A ref field: the record is visible when the record it points at is (and that model's own rules apply, up to 4 hops, no cycle). Tasks follow their project, tiles their dashboard. |
| `public_field` | string | A bool field (stored or computed): everyone who may read the model sees the record when it is true. |
| `shared_with` | string | A text field of user ids separated by commas, spaces or semicolons: those people see the record. |
| `see_all_group` | string or array | The holders of this group (or any of these groups) see every record. |

A record is visible when **any** rule grants it; `platform.admin` sees everything; without `[model.access]` all records of a readable model are visible. Enforcement is in the command layer for every surface (REST, MCP, CLI): `get`, `list` and `search` (the filter is applied before the user's filter, order, page and count, so `total` counts only visible rows), `update`, `delete`, actions (and the actions effects start), the referrers a refused delete names, and ref validation: a record the caller cannot see answers **exactly like one that does not exist** (`no hr_leave_request <id>`; a ref to it fails as a ref to nothing). An agent has its owner's identity and the intersection of its owner's groups and its scopes. On create a person who cannot see the result is refused (`would not be visible to you (set employee to yourself)`), and an update cannot move a record out of the caller's own sight. `/api/meta` reports the rules under `access`, and the backoffice says so on an empty list.

```toml
[[model]]
name = "hr_leave_request"
read_group = "hr.user"
write_group = "hr.user"

  [model.access]
  own = "requested_by"
  see_all_group = "hr.manager"
```

Limits, stated plainly: the rules are about whole records, not fields (use `group` for columns); aggregates and computed totals ignore them, so give the children `via` the parent; uniqueness is global, so a refused duplicate reveals that some record holds the value; a record whose owner field is empty or is not a login is visible to the managers only; existing records get no owner when a model gains `own`.

## 4. Field kinds and their JSON

Values travel as JSON on REST and MCP, and as `key=value` text on the CLI (which converts `number`, `money` and `bool` using the metadata; a decimal or an instant stays text, exactly as written). The server validates every create and update (HTTP 400, `<model>.<field> must be <description>`); existing records are not re-validated.

| Kind | JSON | Notes and example |
|---|---|---|
| `text` | string | One line. `"MBP-0042"` |
| `longtext` | string | Multi-line (the UI shows a text area). `"line 1\nline 2"` |
| `number` | integer | No floats. Use it for counts, minutes, basis points. `1020` |
| `money` | integer | Minor currency units, never a float or a numeric string. 12.34 EUR is `1234`. The package does not store a currency unless you add a field for it. |
| `date` | string `YYYY-MM-DD` | Must be a real calendar date (leap years honoured). `"2026-01-31"` |
| `datetime` | string, RFC 3339 | An instant with microsecond precision (D72). Input needs an explicit offset or `Z`: `"2026-03-29T03:30:00+02:00"`; a value without one, an impossible date, a leap second (`:60`), more than 6 fractional digits, a lower case `t` or `z`, and years outside 0001 to 9999 (after the offset is applied) are refused. It is stored and answered in UTC with six fractional digits, always: `"2026-03-29T01:30:00.000000Z"`, so the stored strings sort exactly as the instants do and two spellings of one instant are one value. Not a JSON number. |
| `decimal` | string | An exact decimal with declared `precision` and `scale` (D72), sent and answered as a canonical string with exactly `scale` fractional digits: `"12.3400"`, `"-0.05"`, `"7"` at scale 0; never a JSON number (a number, including `12`, is refused with `must be ... not a JSON number`). Input is digits with an optional minus sign and an optional point between digits; refused: `+`, exponents (`1e3`), `NaN`, hexadecimal, a decimal comma, thousands separators, spaces, `.5`, `5.`, more fractional digits than the scale (even zeros), and more digits than the precision. Fewer digits are padded (`"12.3"` is stored as `"12.30"`), `-0` is `0`. A field with `round` rounds extra digits only when imported. The OpenAPI schema is `type: string` with a `pattern`; filters compare it as a number, so `"9.5" < "10.25"`. |
| `bool` | `true`/`false` | |
| `select` | string | One of `options`. `"in_stock"` |
| `ref` | string | The id of an existing record of model `to`; a dangling id (or one the caller cannot see, 3.9) is refused. `"9f3c..."` |
| `sequence` | string | Assigned by the server, read-only. `"INV-2026-00042"` |
| `image` | string | The id (64 hex characters) of a stored picture. Upload with `POST /api/blobs?field=model.field` (raw body), then put the returned id in the field. PNG, JPEG, GIF and WebP only; re-encoded, EXIF removed; SVG refused (D60). Not allowed in public submit fields, and not on personal data when public. |
| `file` | string | The id of a stored file from a fixed allow-list (PDF, office documents, text, archives, images). Same upload route; sniffed, never served inline. |

A `[model.public]` table may also set `server_read` (fields the web shop reads from a non-public model, e.g. discount codes) and `server_adjust` (numeric fields the shop changes atomically, e.g. `stock_quantity`); neither exposes the model to visitors (D61).

### 4.1 `list`, `search` and `delete`

`<model>.list` and `<model>.search` take the same optional input; `list` returns a bare array of rows (as before, with no input it returns every record in insertion order), `search` returns `{"rows": [...], "total": N}` where `total` counts the matches *before* `limit` and `offset`.

```json
{
  "filter": [ {"field": "stage", "op": "in", "value": ["new", "qualified"]},
              {"field": "expected_revenue_minor", "op": "gte", "value": 100000} ],
  "order":  [ {"field": "expected_revenue_minor", "dir": "desc"}, {"field": "title"} ],
  "limit": 50, "offset": 100
}
```

- `filter`: conditions `{field, op, value}`, ANDed (at most 50). Ops: `eq`, `ne`, `contains` (text and longtext only, case-insensitive substring, no wildcards), `in` (an array of at most 1000 values), `gt`, `gte`, `lt`, `lte` (not for `bool` or `ref`), `is_null` (`value` true, the default, or false). `ne` also matches records where the field is empty; the other ops never match an empty value. Values must have the field's kind (`number` and `money` as integers, `date` as `YYYY-MM-DD`, `decimal` as a string and `datetime` as RFC 3339 with an offset or `Z`, a `select` value must be an option, a `ref` is an id) and comparisons are typed: numbers numerically, decimals exactly (a filter value may have more digits than the field's scale: `price gt "9.995"` is fine), instants as points in time whatever offset they were written with, dates chronologically, selects in option order, text bytewise in a filter (`contains` ignores case). Text in an `order` is sorted by Unicode collation, see `lang` below. `id` can be used as a field.
- `bucket` and `zone` on a condition (`date` and `datetime` fields, D72): `{"field": "booked_at", "op": "eq", "bucket": "month", "zone": "Europe/Stockholm", "value": "2026-04"}` compares the *period* the field falls in, cut on the wall clock of the zone, with `value` the period's label: `day` and `week` (the Monday) are `YYYY-MM-DD`, `iso_week` is `YYYY-Www`, `month` is `YYYY-MM`, `year` is `YYYY`; labels sort in time order, so `gte` and `lt` work. `zone` is an IANA name (default the platform zone, `OBOP_ZONE`, UTC unless set) and only applies to a datetime. This is the explicit "date in zone" conversion: a date never compares with an instant otherwise (`booked_at eq "2026-03-29"` is an error). Weeks start on Monday.
- `order`: up to 5 `{field, dir}` with `dir` `asc` (default) or `desc`; empty values sort last in both directions; ties end at the record id, so the order does not depend on how the backend returned the records. Text and longtext fields sort by the Unicode collation algorithm with the CLDR tailoring of `lang` (`icu_collator`, compiled in): letters first (`a`, `A` and `á` tie), digit runs by value (`item 2` before `item 10`), then the exact text. German, Swedish, Finnish and Polish therefore sort `ä`, `å`, `ö` and `ł` as their dictionaries do, and the browser (`Intl.Collator` with `numeric` and base sensitivity) sorts the same way; both sides are tested on the same fixtures (`web/apps/backoffice/src/collation.fixtures.json`). Ids and refs sort bytewise, numbers, money and dates by value.
- `lang`: a BCP-47 tag (`sv`, `de-AT`, at most 35 characters) that picks the collation; without it the language-neutral root order applies. An invalid tag is an error.
- `limit`: 1 to 5000 (default: no limit); `offset`: 0 or more.
- Unknown keys, ops or malformed values are an error (HTTP 400, for example `unknown op "like" (use eq, ne, contains, in, gt, gte, lt, lte, is_null)`).
- Filtering and ordering happen before masking and withholding, so they are restricted to fields the caller can read in clear: a field the caller's groups do not include, a secret field, and for an agent any personal-data or sensitive field (which it sees only as a token or not at all) are refused with exactly the error of a field that does not exist, `unknown field <model>.<name>`. A search can therefore not be used to guess values the caller may not see.

- `aggregate` (`search` only): returns groups and totals computed on the server instead of rows, so a pivot, a chart, a board's column counts or a dashboard tile never needs the table in the browser. `{"filter": [...], "aggregate": {"group_by": ["state", {"field": "due", "bucket": "month"}], "measures": [{"op": "count"}, {"op": "sum", "field": "amount_minor"}]}}` answers `{"rows": [], "total": N, "groups": [{"key": {"state": "won", "due": "2026-10"}, "count": 3, "measures": {"sum:amount_minor": 4500}}]}`. `total` is the number of records the filter matches. `group_by` takes at most 2 fields (text, select, bool, ref, number, money, decimal; a date or datetime must carry a `bucket`: `day`, `week` (the Monday), `iso_week`, `month` or `year`, and a datetime may carry a `zone`, an IANA name, default the platform zone, so `{"field": "booked_at", "bucket": "month", "zone": "Europe/Stockholm"}` puts `2026-03-31T22:30:00Z` in `2026-04`); an empty value groups as `null`. `measures` takes at most 8 entries: `count`, and `sum`, `min`, `max` of a number, money or decimal field, and `avg` of a decimal field (a sum of nothing is 0, a min or max or avg of nothing is `null`; a sum that does not fit 64 bits is an error). A decimal measure is an exact decimal string: `sum`, `min` and `max` are exact in whole units of the field's scale (128 bits), and `avg` is the exact sum divided by the number of values and rounded once to the measure's `scale` (default: the field's, 0 to 12) with its `round` (`half_even` by default, `half_up` or `down`): `{"op": "avg", "field": "price", "scale": 4, "round": "half_up"}`. Groups come ordered by their keys (select option order, text by the `lang` collation, empty last). It takes `filter` and `lang`, not `order`, `limit` or `offset`, and is refused by `list`. Every field it names obeys the same access rules as filter and order: one the caller cannot read in clear answers `unknown field <model>.<name>`, so no group, count or sum can leak a value. It is computed in one streaming pass over the model's records (`Backend::scan`: a record at a time on memory, SQLite and PostgreSQL), so time is linear in the records and memory is `groups * measures`; more than 10,000 groups is an error that asks for a narrower filter, never a silent cut. It is not pushed down to PostgreSQL JSONB: the filter semantics (typed comparisons, option order) would have to be duplicated in SQL and a wrong whitelist there would be a data-exposure bug, so the portable evaluation is the one implementation.

`<model>.delete` takes `{"id": ...}` and returns `{"id": ...}`. It needs the model's `write_group`; an agent only gets an approval plan. It is refused while another record has a `ref` to the record: `cannot delete party P: still referenced by accounting_move (3): id1, id2, id3; sale_order (1): id4` (the first five ids of each model).

Conventions the bundled packages follow: refs are named `<thing>_id`; time of day is a `number` of minutes since midnight (540 is 09:00) next to a `date`; rates are `number`s in basis points (2000 is 20.00 percent); numbers such as invoice numbers are `sequence` fields; yes/no with more than two states is a `select`.

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

### Assigning groups to people (D69)

The permission groups your module names (on models, fields and actions, and `see_all`) are exactly the groups an administrator can give to a person: Customize > Security > People lists every group of every installed module, and `obop admin groups USER --groups a,b` and `POST /api/auth/users/{name}/groups` accept only those. Install the module first; a group no module declares is refused. Removing a module does not take its group from people who hold it (they keep it until an administrator changes it), but it can no longer be given to anyone else.

`personal_data = true` marks a field as about an identifiable person; it drives access and erasure requests (D33). A personal-data field may never be `none` (the gate refuses it). A field can be sensitive without being personal data (a company's bank balance is `financial`, not personal).

### Groups

A group is just a name `module.role`; it exists as soon as something refers to it. Conventions: `<id>.user` (day-to-day), `<id>.manager` (configuration and approvals); add a third group (`approver`, `finance`, `special_reader`) only when a separation of duty is real: someone who enters a thing must not be the person who releases it. The administrator group `platform.admin` satisfies every check for a human; an agent token only has the concrete groups listed in its scopes, and can only narrow its owner's groups.

Three places take a group:

1. Model `read_group` / `write_group`: read (`get`, `list`, `search`) and write (`create`, `update`, `delete`).
2. Field `group` (default: the model's read group): who sees the field. A field the principal cannot read is not described to it in `/api/meta`.
3. Action `group` (default: the model's write group): who may run the step. This is how you require a stricter group for posting, paying, closing.

## 6. Workflow actions and approvals

An action is the only custom behaviour X1 offers: a guarded assignment `field := to` valid when `field == from`. Chain actions on one `state` select to build a workflow; you may declare several actions with different `from` values (for example `refund` from `paid` and `refund_delivered` from `delivered`), and several actions can lead to the same state. Action names must be unique per model, so use distinct names for the two refunds.

What the dispatcher does with `approval = true`:

- A **human** who holds the action's group runs it directly and it takes effect at once.
- An **agent** (a principal made by `agent-token`, or an MCP client) that calls it gets `ApprovalRequired { plan_id }`; nothing changes. A human reviews and approves the plan (`POST /api/approve/{plan_id}`, or the approval prompt in the UI), and only then the action runs, as the human.
- Agents may run actions without `approval` directly, within their scopes.

Mark `approval = true` on anything irreversible or money-moving: posting to a ledger, paying, refunding, sending to customers, publishing publicly, closing a period, retiring an asset. Reversible bookkeeping steps (assign, schedule, handle) normally do not need it.

What actions cannot do: set more than one state field, notify or call out (effects, 3.8, change records in the same database only). When an action runs it also checks that the record has every `required` field set (the error names the field, for example `helpdesk_ticket.subject is required and must be set before state can change to "in_progress"`).

### 6.1 Governed fields: actions are the only way to move a state (D48)

A field that is the `field` of at least one action is *governed*. The generic `create` and `update` cannot set it freely, so the stricter group and the `approval` flag of an action cannot be bypassed:

- **create**: leave a governed field out (it becomes the first option of its select) or send exactly the first option. Anything else is refused: `helpdesk_ticket.state starts as "open" when a record is created; it moves on through the model's actions`. A value that is not an option at all gets the ordinary `must be one of: ...`.
- **update**: setting a governed field to the value it already has is not a change. Changing it from `a` to `b` is accepted only as an implicit run of a declared action: there must be an action on that field with `to = b` and `from` unset or `= a`, and the caller must hold *that action's* group. If the action has `approval = true`, an agent's update returns the usual approval plan (`202`, `plan_id`), and a human who holds the action's group approves it; the action then really runs (side effects such as the invoice number included). A change with no such action is refused and says where the record can go: `helpdesk_ticket.state cannot change from "open" to "closed"; from "open" it can only go to: in_progress`, or `... no transition leaves "closed"`. A caller without the group is told which action and group: `... moving from "draft" to "posted" is the post action, which needs the accounting.approver group`.
- Other fields of the same `update` are validated as before, all before anything is written, so a mixed update applies completely or not at all. The record's required fields must be set for a transition to run.
- When several actions lead to the same state the caller uses one it may run (an exact `from` before an open one). This is how the UI's kanban drag and status-bar clicks keep working: they send `update` with the new state, and succeed exactly when the person could have pressed the corresponding action button.
- A proposal is checked again when it is approved, because the record may have changed since.
- Data written outside the commands (seeds, imports straight into the store) is not checked.

The built-in modules follow the same rule: `lead.stage` (actions `qualify`, `mark_won`, `mark_lost`, `reopen`), `sale_order.state` (`confirm`, `cancel`, `reset_to_draft`) and `invoice.state` (`post`, needing `invoicing.manager` and approval, and `cancel`).

## 7. Refs, dependencies, installing, upgrading, removing

- A `ref` to a model in your own package is verified when the file is parsed. A ref to any other model (built-in `party`, `product`, `sale_order`, `invoice`; or another package's) needs that model to exist at install time and the module that owns it among your dependencies. Built-in module ids to use: `base` (party), `product`, `sales` (sale_order), `invoicing` (invoice), `crm`. Packages: their own id (`hr` for `hr_employee`).
- **A plan comes first (D55).** Every install and upgrade computes a plan before it changes anything: is the core right, is every dependency present at a version that satisfies its requirement (built-ins against the core, installed packages, marketplace packages), in what order must modules be installed. `module_plan.preview` returns it (give `id` for a marketplace package or `manifest` for an uploaded file); the Modules page shows it in a dialog; `obop modules plan FILE|ID` prints it. It is read-only and an agent may call it.
- **Dependencies come with the module.** A dependency that is not installed but is in the marketplace is installed first, transitively, in dependency order; one that is installed at a version that fits is skipped; one that is installed but too old is upgraded when the marketplace has a version that fits. Cycles, a missing dependency, a core mismatch or a conflict are **blockers**: the plan lists them and nothing is installed.
- **Install** (`marketplace_listing.install`, `module.install`, `obop modules install`) applies the plan as one operation: validate, run the gate, check collisions, register, store. Every step records who installed it and when. If a step fails the earlier steps are undone and the error says so. An agent proposes the whole plan; one human approval runs it once. `without_dependencies: true` is refused.
- **Update (D63)**: an installed package is updated to a **higher** `version` that the marketplace offers (`module.upgrade ID`, `module.upgrade_all`, `obop modules update ID|update-all`, Customize, Modules, Updates) or that you upload (a file with the same `id`). The plan shows the release notes, what changes in the models, the data steps that will run, and which installed modules depend on this one with the range each supports. It is applied as **one atomic batch**: the data steps and the new package record are written together, so a failure or a record changed meanwhile leaves everything as it was. It holds the cross-process lock of D59, so two server processes cannot update at once.
  - **Additive changes** (a new model, an optional field, a select option, an action) need nothing: records keep their data and the new parts start empty.
  - **Breaking changes** are refused with a readable message unless the new version declares a step for them (section 7.1): a removed field, a retyped field (never convertible: add a new field and move the data yourself), a removed select option, a removed model, and a new or newly required field while records exist.
  - An update is blocked when an installed module supports a range of this one that the new version leaves (`projects` 2.0.0 while `timesheets` needs `<1`).
  - **Rollback** (`module.rollback ID`, Versions tab) puts back the version before the last update, one level deep. Renames are reversed; fields the update added stay in the database and the older version ignores them. It is refused when the update deleted data (a `drop_field` step: restore the backup instead), when the old version no longer fits the core or its dependencies, or when it would leave a dependent's range.
  - Agents may only propose `module.upgrade`, `module.upgrade_all` and `module.rollback`; each is audited in `security_event` (`module_update`, `module_rollback`, with the data steps named).

### 7.1 `[[migration]]`: declarative data steps (D63)

```toml
[[migration]]
from = "<0.2.0"            # the installed versions this step applies to (a range, required)
op = "rename_field"        # rename_field | set_default | drop_field
model = "assets_asset"
field = "plate"            # old name (rename), the field filled (set_default), the field dropped
to = "registration"        # rename_field only: a field of this version
description = "optional sentence for the plan"

[[migration]]
from = "<0.2.0"
op = "set_default"         # writes `value` into every record where the field is empty
model = "assets_asset"
field = "tier"
value = "standard"         # text, whole number, true/false; a select value must be an option

[[migration]]
from = "<0.2.0"
op = "drop_field"          # deletes the values of a field this version no longer has
model = "assets_asset"
field = "notes"
confirm = true             # required: the values are deleted and the step cannot be rolled back
```

Steps are checked when the package is read (the model and fields must exist in this version, the value must fit the field kind) and run in declaration order, inside the same atomic batch as the update. Each record is changed with a compare-and-set on the value that was read, so an edit made by a user meanwhile makes the whole batch fail cleanly and it is retried against the fresh data. With no records in a model, none of its changes can leave anything invalid, so nothing is refused for it.
- **Uninstall** (`obop modules uninstall ID`, Customize, or `module.uninstall`; an administrator only) removes the commands and the models from the app; the **data stays** in the database, and installing the module again brings every record back. A module cannot be uninstalled while another installed package depends on it; the error names the dependents, directly and through others.
- A package that no longer fits the running core (after the instance was upgraded) still loads, so its data stays reachable; the Modules page marks it incompatible.
- Install, upgrade and uninstall are `platform.admin` commands that agents may only propose.

## 8. Testing and publishing

### Test a package

- `cargo run -- modules install FILE --db sqlite:///path/x.db` runs exactly the checks of an upload: parse, validation, the D41 gate, dependencies, refs. It prints the models and groups it created, or the error. Use a throwaway database; install dependencies first with `modules install-from ID` (for bundled packages).
- `cargo run -- modules check [--catalog] FILE...` needs no database: it parses each file (every key of section 3.1), runs the D41 gate, and applies the install rules that need no database: the id is not a built-in module, every dependency is a built-in module or a bundled package, every range is satisfiable by what exists (built-ins against the core version, bundled packages against their versions), no cycle, and every `ref` to a model outside the package points at a model of the dependencies. A `requires_core` the running core does not meet is a warning for a file you are preparing and an error with `--catalog`, which also demands what the bundled catalogue and the marketplace require (`license`, `requires_core`). It prints `ok  FILE`, `warn  FILE: ...` and `FAIL  FILE: <error>`, and exits non-zero if any file fails. `cargo run -- modules check` without files runs the gate over the linked built-in modules.
- `cargo run -- modules list` shows every module with version, licence, core requirement and source; `modules tree [ID]` prints the dependency tree of a module (a marketplace id works too) or of every installed package; `modules plan FILE|ID` prints the install plan without installing.
- Customize app, **Module builder**: define models, fields (name, description, kind, options, target), and actions in a form; it writes the `obop.toml` for you and installs it. Customize, Modules, Marketplace, **Upload your own package** accepts any `.toml` file (or pasted text), shows the error or the plan as you go, and installs through the same plan dialog as the marketplace.
- To exercise a package like the bundled ones, copy the pattern of the `bundled_packages` tests in `crates/obop/src/main.rs`: install through the real installer, create a record per model from the metadata, run every action in and out of order, check agents are only given a proposal for approval actions.
- Try as an ordinary user: `echo "a-long-password" | cargo run -- admin create alice --groups assets.user --db sqlite:///path/x.db`, then log in and see which fields and buttons appear.

### Publish

The marketplace is a static host. Serve over HTTP or HTTPS:

```
index.json                 [{"id":"assets","name":"Assets","version":"0.1.0","description":"...","author":"Example Ltd",
                             "summary":"...","license":"Apache-2.0","requires_core":"^0.2","maturity":"beta",
                             "categories":["productivity"],"keywords":["assets"],
                             "dependencies":{"base":"^0.2"}}, ...]
packages/assets.toml       the obop.toml of each package listed
```

`index.json` is an array of objects with `id`, `name`, `version`, `description` and optional `author`. Since D55 a listing may also carry the catalogue metadata of the manifest, so the Modules page can show cards, badges, compatibility and dependencies without fetching every manifest: `summary`, `authors` (array), `license`, `homepage`, `repository`, `documentation`, `support`, `keywords` (array), `categories` (array), `icon`, `requires_core`, `maturity` and `dependencies` (an object, module id to requirement). Every key after `author` is optional and unknown keys are ignored, so an index written before D55 keeps working (its modules show without badges). An entry whose `id`, `version` or any requirement does not parse is left out rather than failing the whole catalogue. The manifest remains the authority: an install fetches it and checks it exactly as an upload, and for a marketplace package `license` and `requires_core` must be declared in the manifest itself. Package ids may contain only lowercase letters, digits and `_` (anything else is refused before a request is made). Point an instance at it with the environment variable `OBOP_MARKETPLACE_URL=https://host/path` (no trailing slash needed). The instance shows the bundled catalogue first and your host after it; if the same id exists in both, the bundled one wins. Every download is validated exactly like an upload, and responses are limited to 1 MiB. The bundled catalogue itself is `packages/*.toml`, compiled into the binary by `crates/kernel/obop-pkg/src/marketplace.rs`. Package signing and paid listings are not built (D45).

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

- **Computed fields are small on purpose.** Integers, exact decimals, text, booleans and instants; sums, counts, minimums and maximums over child records; no floats, no date or instant arithmetic (an SLA "elapsed" cannot be computed: there is no difference of two instants yet), no function that turns an instant into a date in a zone (use a filter or aggregate `bucket` with a `zone`), no aggregate over grandchildren, no joins. They are computed when read, so they cost a look at the children, cannot be indexed, and sorting by one reads every row. They ignore row-level access (3.9).
- **Sequences are per database and per process.** The counter is transactional, so numbers are gapless against failures and parallel calls inside one server (the dispatcher holds one lock for writes of models with rules). Two servers on one PostgreSQL database could still race on a counter; one process per database is the rule (D30). A gapless series can still have a hole if somebody edits the database by hand. Existing records are numbered only by `obop sequences backfill`.
- **Uniqueness is global per model** (optionally scoped by `unique_with`); it is enforced on every command write and by a database index where one can be created, not on seeds or imports, and it reveals that a value exists. Case is significant.
- **Row-level access is by whole record**, in the command layer; see 3.9. It does not hide a record from aggregates or from computed totals, does not apply to the public site layer (D50, its own whitelist), and does not apply to data written outside the commands.
- **Effects change records in the same database only.** They do not send email, call a payment provider or run on a schedule; the email, sign, eCommerce and POS packages record what happened or should happen. Effects need the caller to hold the groups of everything they touch (so confirming a sale needs `invoicing.user`, receiving a purchase `inventory.user`); at most 12 per action, 500 records per operation, 4 nested actions; an effect cannot set a governed field except through its action, and plain create and update never run effects.
- **Governed state, but not locked records.** A state field is protected (section 6.1), but every other field of a posted or signed record can still be edited by anyone who holds the `write_group`; "immutable once posted" is not enforced (a numbered record cannot be deleted, though, and a posted invoice keeps its lines only by convention). Data written outside the commands bypasses the rules. Nothing stops two servers racing on one PostgreSQL database between the check and the write of a transition (one server serialises them).
- **Delete is plain**: `delete` removes the record for good and refuses while another record refers to it through a `ref` (the error lists the referencing models, counts and up to five ids of records the caller can see) or while it holds a gapless number. There is no cascade, no soft delete and no tombstone; the check sees only models that are installed now. Personal-data erasure still uses the dedicated `erase` command of the owning module (`party.erase`), which anonymises in place and keeps the references valid; `delete` is not a substitute for it. There is no validation across records beyond assertions on actions.
- **No migrations**: see section 7. Turning a free text field into a `sequence`, or adding `own` to a model, does not touch old records.
- **Search is evaluated in memory**: `list`/`search` filter, sort and page on the server (section 4.1), but the server reads the model's records from the store to do it, on all three backends, so a very large table is slow; there are no indexes beyond the unique ones, no full-text search and no grouping or aggregation on the server. Filtering and sorting on a field the caller cannot read in clear (not permitted, withheld, or masked for an agent) is refused.
- Limits: 256 KiB for the whole document including its translations, 50 models, 200 fields per package; no floats; one state field per action; at most 50 filter conditions, 5 order keys and 5000 rows per call.

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

**Where do I put a model that belongs to two apps?** Models are named after the module that owns them. Add the module that owns the model to `[module.dependencies]` (with the versions you tested) and `ref` to it.

**Can a module have no models?** The file parses with no `[[model]]`, but it adds nothing.

**Where are decisions recorded?** D11 (tiers), D45 (packages and the admin), D46 (backoffice and Customize), D47 (typed fields), D48 (governed fields), D49 (delete and search), D56 (effects and assertions), D57 (computed fields, uniqueness, sequences), D58 (row-level access) in `docs/DECISIONS.md`; data classes in `docs/CONVENTIONS.md` section 5.

## 12. The bundled workflows

What the bundled modules do with sections 3.8 and 3.9 (each is exercised by a test in `crates/obop/src/model_tests.rs`):

| Module | Does |
|---|---|
| `sales` (built in) | `sale_order` has lines (`sale_order_line`); its `total_minor` is computed from them and `number` is `SO-00042`. `confirm` creates a draft `invoice` with a copy of every line (the caller needs `invoicing.user`). |
| `invoicing` (built in) | `invoice_line`; `total_minor` computed; `post` is refused for a total of zero, needs `invoicing.manager` and approval, and assigns `INV-2026-00042` from a gapless yearly series. |
| `product` (built in) | `sku` is unique. |
| `accounting` | An entry shows `total_debit` and `total_credit`; `post` is refused unless they are equal and assigns `JE/2026/00042` (gapless, yearly). Account and journal codes are unique. |
| `inventory` | `validate` (stock managers, approval) subtracts the quantity from the item at the source location and adds it to the item at the destination, creating missing items; one item per product, location and lot; moves are numbered `MV-00042`. |
| `purchase` | Orders are numbered `PO-00042`; the total is the sum of the line amounts; `receive` creates a draft stock move per line to the receiving location (the caller needs `inventory.user`). |
| `pos` | Subtotals and the order total are computed; `pay` is refused until the payments cover the total and creates draft stock moves from the order's stock location; receipts are numbered gapless per year. |
| `expenses` | Claims are numbered `EXP-2026-0042`; an employee sees only their own, managers and finance see all. |
| `hr` | Leave requests: employees see their own (`requested_by`), `hr.manager` sees all. |
| `documents` | Confidential and restricted documents are visible to their owner and `documents.manager`; the rest to every reader (`open_to_all` is computed from `confidentiality`). |
| `timesheets` | Entries are private to their worker; `timesheets.manager` sees all. |
| `projects` | `progress` (percent of non-cancelled tasks that are done) and `estimate_minutes` are computed. |
| `helpdesk` | Tickets are numbered `TCK-00042`. |
| `dashboards` | A dashboard is visible to its owner, to `dashboards.manager` and, when `shared`, to every reader; its tiles follow it. |
