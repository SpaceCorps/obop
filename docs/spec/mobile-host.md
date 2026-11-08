# The phone host app and the Mileage module: what a host needs from Mileage (PROPOSED)

> Status: PROPOSED. This is the contract a phone host app needs from obop's server and from the Mileage app, and it has to be agreed before anyone builds a host. Nothing here is decided, built or run on a phone as a host. Terms: the host is the one phone app that holds obop's apps as modules; an implementation is an organisation that builds a host on obop for its own use; Mileage is the first module.
>
> Written against: `main` = origin/main at fe1b93a; `ios` = origin/feat/mileage-ios at 89c4e50 (PR 34, which contains PR 27); `sv` = origin/feat/mileage-sv at 5bdd17d (PR 45), read only for the Swedish items.
>
> Follow-up edit, 2026-11-09 (documentation only): the records of section 0 are entered in `docs/DECISIONS.md`, the rows of section 10 that carry the codes H, M and P are applied, and the table of section 10 says which rows are applied and which wait. Every row was found again by its text on origin/main at 9ba3afed, and the register lines named below were opened there; all other citations in this document still name the refs above. The sentences changed by that edit are the ones that said the records will be entered, the two statements that the direction is not recorded on main, the status of PR 51 (merged since), and the table of section 10.
>
> Citations: `main:path:line`, `ios:path:line`, `sv:path:line`. `app/` stands for `web/apps/mileage-app/`. Server paths omit the leading `crates/kernel/`; the Mileage module is `crates/modules/mileage/`. A path without a prefix under `docs/` or the server crates is on `main`; a path under `app/` or `web/` is on `ios` unless the text says `main`. A bare `web/packages/client/src/oauth.ts` is on `ios` only; it is not on main.
>
> Marks: "verified" means the line was opened on the named ref while this document was written and says what the sentence claims; nothing was run. "Not verified" means it was not opened, or cannot be opened from the repository. "Reported" means it comes from a build report given to the project and was not reproduced here.

## 0. Status and scope

Owner direction of 2026-11-03, reported to the project second-hand, through another session, and not confirmed by the owner; it is entered in the decision register only as a relayed item, not as a ruling (OR-39, in the list "Relayed, not yet confirmed by the owner"; see Records below): the phone client is to be one app that holds obop's apps as modules, Mileage the first. This document reads that as a host app with the Mileage app inside it. The reading is PROPOSED. The host design is not ruled. The direction was not recorded on main when this document was written: `main:docs/DECISIONS.md:1269` said that the design of the host app is a separate proposal and is not described there, and `main:docs/DECISIONS.md:233` and `:1646` said that Q63 is reserved for the host questions (at 9ba3afed, before the follow-up edit, those lines are 1276, 236 and 1672).

Not ruled. All of this stays PROPOSED or open:

1. The shape of the host: one generic codebase that each organisation builds, or a library with a thin target.
2. Where an implementation's registry data and build live.
3. The application id, the store accounts and the signing keys. Q37 is open and one-way (`main:docs/DECISIONS.md:1621`).
4. The module descriptor of section 2, module-qualified routes and the split of the runtime.
5. Whether the standalone Mileage binary stays buildable after the host exists.
6. D87's scope limits and their wording. They are PROPOSED (`main:docs/DECISIONS.md:241`).
7. Whether the phone is outside the UI-parity rule (OR-16, D80). No obop record of it exists on main, and `CLAUDE.md` says nothing is exempt until the owner approves it.
8. Every server change in section 6.

Records. The direction is entered as OR-39 and the host design as D93 (PROPOSED); the host questions use the Q63 that was reserved for them (a provisional number). They are entered in `docs/DECISIONS.md` by the follow-up documentation change of section 10, because open pull requests edit the register lines. OR-39 stands only in the separate list "Relayed, not yet confirmed by the owner" and in none of the ratified lists: it is not a ruling, and it becomes one only on the owner's own word. D92 was taken by draft PR 51 (`origin/feat/sign-signer:docs/DECISIONS.md:232`) when this document was written; PR 51 is merged since (09861505), and D92 is on main at `docs/DECISIONS.md:232` (9ba3afed).

The one wording. Every document that mentions the direction uses this sentence, defined here once: "The phone client is planned as one host app that holds modules, Mileage the first (Proposed: a direction relayed to the project and not yet confirmed by the owner; D93, spec/mobile-host.md)." Only the form of the link changes with the document. No document says that the owner ratified, approved or decided the host direction or the wording of D87's limit.

What this document is not: it is not an implementation, nothing was run on a phone as a host, and there is no store submission. It adds one file and edits no existing line. PR 27, 34 and 45 are open drafts of the standalone Mileage app (Android, iOS, Swedish). They are stacked: 27 is an ancestor of 34, and 34 is an ancestor of 45 (checked with `git merge-base --is-ancestor`). PR 51 was an open draft of another feature (signing a document) that edited some of the same documents; it is merged since (09861505). This document does not collide with any of them. Section 10 lists the lines that go stale and why they are edited later.

## 1. The split in one picture

Today everything is one app. The split in the third column is PROPOSED. "Today" cites `ios:app/`. The seam is partly there. No non-test file in `app/src/core` imports react, react-native, expo or a Node built-in module; it imports only relative files and the workspace packages `@obop/client` and `@obop/trip-core` (git grep). Every Expo or React Native import is in `src/native`, `src/ui`, `App.tsx`, `index.ts` or `app.config.ts`.

| Concern | Owner today (standalone app) | Owner in the host (PROPOSED) | Change needed |
| --- | --- | --- | --- |
| Session and sign-in | `src/core/session.ts` (refresh, `needsRegistration` at :27-33); `src/native/auth.ts:132-196` (register, system browser at :168, exchange, bind); `web/packages/client/src/oauth.ts:217-406` (ios only; register, PKCE, exchange, refresh, revoke) | Host | Lift session, auth, keychain, secure-storage and platform-deps into packages. They hold no trip logic, with one exception: `auth.ts` imports `Binding` and `Owner` from `core/identity.ts` (`auth.ts:17`) and calls `host.bindIdentity` (`auth.ts:181`). They import Mileage-named constants from `src/native/config.ts`: `CLIENT_NAME`, `REDIRECT_URI` and `SCOPE` (`auth.ts:25`) and the key names `SERVER_KEY_NAME` and `TOKENS_KEY_NAME`, whose values contain `mileage` (`secure-storage.ts:3`, `config.ts:18-19`). Replace them with one identity object the host builds. |
| Device record | `src/native/devices.ts` (device.get, update, revoke); owner question at `src/native/auth.ts:87-101` | Host owns the calls; the module receives the device id and the owner | `removeThisDevice` ends in `leaveSession`, which stops the Mileage recording and clears the Mileage outbox (`src/native/devices.ts:75-81`, `src/native/actions.ts:140-147`). The host version must call each module's sign-out hook. |
| Encrypted database and key | `src/native/database.ts:20-32` (one file `mileage.db`, 32 random bytes as key); `src/core/open-outbox.ts:123-127,155-190` (cipher check at :123-127. On iOS only, a file that a key made in this start, or one whose key is still stored as `pending:`, cannot open is deleted and replaced, `:160-185`. A proven key is never followed by a deletion and the error is shown, `:18-23`. On iOS a fresh database also drops the stored session, `:188-190`) | Host owns the key, the open procedure and the recovery rule; each module gets a handle | Decide one file or one file per module (open question, section 9.1 item 4). In the case above the recovery rule deletes the whole file (`open-outbox.ts:182`), so modules that share a file lose data together. `SCHEMA_VERSION` is one number (`src/core/sql-store.ts:19`); it needs a version per module. |
| Sync and outbox transport | `src/core/sync.ts` (timer, retry, signed-in gate) is typed on the Mileage `Engine` (`sync.ts:1,73`); wired at `src/native/runtime.ts:106-111` | Host owns scheduling and the signed-in gate; each module owns what it sends | Make the controller call a `drain` port. Server side, only `bulk_create` has idempotency (6.5). |
| Location prompts, plist and manifest declarations | `app/app.config.ts:131-132,143-161` (one local-network text, one location text naming "obop Mileage"); `src/native/permissions.ts:47,75`; `src/ui/screens/Permissions.tsx:19,76-77` | Host owns the one Info.plist and manifest; modules contribute fragments | The texts name the app, not a module. The host config needs a place for module-contributed plugin options. See section 5. |
| Background location task and Android foreground service | `src/native/task.ts:22` (`defineTask`); `index.ts:3` (imported before the app starts); `src/native/recorder.ts:30-42` | Module owns the task body and start/stop; host owns the entry file and imports each module's task file at top level | The task name is one constant (`src/native/config.ts:14`). The descriptor lists the task files (section 2). |
| Navigation and module list | `src/ui/router.ts:9-83` (a flat `Route` union, `gate`); `App.tsx:47,69-77,113-126` (reducer stack, one hardware Back handler, one switch). No navigation library. | Host owns bar, drawer, stack, Back and the module list; module owns its screens | Routes become module-qualified; a module gets a sub-stack. The `foreign` and `sign_in` gates become host gates. The `recording` gate stays the module's. |
| Locale and theme | `src/i18n/index.ts:5,8` (`LOCALE = "en"`, one catalogue); `app.config.ts:98` sets `userInterfaceStyle: "automatic"` (`web/packages/ui-native` not re-read) | Host owns the language choice and the theme tokens; module owns its catalogue | Catalogue keys need a module prefix; `MessageKey` is the key type of one object (`index.ts:1-5`). The host passes locale and theme in. |
| Build identity | `app/app.config.ts:24-25,90-101,126`; a second copy at `src/native/config.ts:6-8` | Host | Section 3. |
| Declared groups at registration | `src/native/config.ts:11` (`SCOPE = ["mileage.driver"]`); `src/native/auth.ts:132-137` | Host: the union of its modules' groups | Section 3. |
| Trip engine | `src/core/engine.ts`, `src/core/api.ts` (`mileage_trip`, `mileage_point` calls) | Module | None, except the sync port and a trip list that is safe for a token that sees other people's trips (the two points after this table). |
| Trip store | `src/core/store.ts`, `src/core/sql-store.ts` (tables trip, point, batch, meta at :24-70); thinning in `src/core/thin.ts` | Module, on a handle from the host | The `meta` table also holds host-level state: the owner record (`src/core/identity.ts:21,81-89`), the disclosure flag (`Permissions.tsx:19`), the stale-file note (`open-outbox.ts:30`). Move these to a host table. |
| Trip outbox | `src/core/api.ts:110-131,152-154` (trip creation and point batches go through `bulk_create` with an idempotency key; update, end, submit and discard at `api.ts:156-181` carry none; the comment at `engine.ts:44-47` says every write of the outbox has one, which the code does not do); `src/core/values.ts:98-102` (`trip:<id>`, `points:<id>:<first>-<last>`); `batch` table | Module; the host asks it for counts | Add the `unsent` hook (section 2). |
| Mileage screens and strings | `src/ui/screens/` (Home, Trips, TripDetail, Recording, Permissions; Account and SignIn are mixed); `src/i18n/en.ts` | Module, except Account and SignIn | Move Account and SignIn to the host and split their strings. |

Two points for the sync port and the trip list:

- A refusal that asks for a fresh sign-in must not be lost. `reauth_required` is catalogued as HTTP 400 (`obop-commands/src/errors.rs:253-254`); no code returns it yet. `classify` turns any other 4xx into `refused` (`src/core/engine.ts:31-63`), and `isSafeToRetry` repeats no status below 500 except 429 and 409 `incomplete` (`ios:web/packages/client/src/errors.ts:262-266`). Once a step-up exists, the `drain` hook and 6.5 need a stated rule for this case.
- `listTrips` sends order and limit and no owner filter (`src/core/api.ts:144-148`). Whether every returned trip is merged into the local store is not traced here. It matters once a token can see trips of other people; a reviewer-class group is blocked on a phone today (section 3).

## 2. The module descriptor (PROPOSED)

The smallest contract a module offers the host, and what the host passes in. The sketch is illustrative. Nothing here is built. Names are placeholders.

```ts
// ILLUSTRATIVE. Not built. Names are placeholders.
interface PhoneModule {
  id: string;                                // the server module id: "mileage"
  groups: readonly string[];                 // groups it needs on a phone: ["mileage.driver"]
  title(locale: string): string;
  icon: string;                              // an icon name; the server module has one ("fleet")
  screens: Record<string, ScreenDef>;        // keys are module-qualified: "mileage/trips"
  entry: string;                             // the screen the module opens on
  migrations: readonly Migration[];          // run by the host on the module's database handle
  backgroundTasks: readonly string[];        // files that call defineTask when imported
  catalogue: Record<string, Messages>;       // keys carry the module prefix
  hooks: {
    onForeground?(ctx: HostContext): void;   // app became active (Recording.tsx:123 does this today)
    onResume?(ctx: HostContext): void;       // the sign-in was restored after a start
    unsent(ctx: HostContext): Promise<{ items: number; label: string }>;
    onSignOut(ctx: HostContext, keep: boolean): Promise<void>;
    drain(ctx: HostContext): Promise<DrainReport>;
  };
}

interface HostContext {
  client: ObopClient;                        // authenticated; throws 401 when signed out
  deviceId: string;
  owner: { origin: string; user: string };
  grantedGroups: readonly string[];          // the `scope` of the token response
  locale: string;
  theme: ThemeTokens;
  db: SqlDb;                                 // the module's handle
  emitChange(): void;
}
```

Notes on the sketch:

- `title` is already served as the module `name` (`obop-mcp/src/lib.rs:383`), and `icon` could come from the server once `/api/meta` serves it (6.2). The server module already has an icon and a category (`mileage/src/model.rs:21-27`). The groups a module needs on a phone have no server source today, and 6.2 does not add one. What only a build can say is screens, tasks, migrations, hooks, the catalogue and the groups.
- The smallest refactor that gets here is a module descriptor, module-qualified routes, the `Runtime` object split into a host part and a module part, and one identity object that holds the application id.

What exists in code (verified on `ios`):

- `app/src/core` is pure TypeScript with ports: `SqlDb` (`src/core/sql.ts`), session storage and OAuth dependencies injected into `createSession` (`src/native/runtime.ts:57-65`).
- A `Runtime` object is the one handle the screens use (`src/native/runtime.ts:15-41`). Its fields mix both sides. Host side: `session`, `client()`, `revokeSignIn()`, `binding()`, `bindIdentity()`, `takeOverOutbox()`, `signOutUntold()`, `setSignOutUntold()`. Module side: `store`, `engine`, `sync`.
- The ideas behind the hooks exist as functions: `unsentOf` (`src/core/identity.ts:55`), `leaveSession` (`src/native/actions.ts:140-147`), the `AppState` listener (`src/ui/screens/Recording.tsx:123`), `session.restore()`.

What does not exist: the descriptor and the registry; module-qualified routes (`Route` is a flat union, `src/ui/router.ts:9`); the split `Runtime`; a module prefix on catalogue keys; a schema version per module; a host sign-out dialog that asks each module; a place in `app.config.ts` for module-contributed plugin options.

Order of the refactor (PROPOSED), so that each step leaves `vp test` green:

1. One identity object, so the application id exists once.
2. Split `Runtime` into a host runtime and a Mileage runtime.
3. Give `createSyncController` a `drain` port instead of the `Engine`.
4. Prefix routes and catalogue keys with the module id.
5. Give the module a database handle and its own version.
6. Add the descriptor, and a registry the host's `index.ts` reads to import task files.
7. Only then move the shared parts into packages.

In this order, steps 1 to 5 are changes inside the Mileage app (PROPOSED reading). Steps 6 and 7 are the ones that choose a way of carrying a module into a host (a library with a thin target, or one generic codebase that each implementation builds). Until that is decided (section 0), nothing should be built that assumes one way.

## 3. Configuration and identity

How each setting works today (`ios`):

| Setting | Read at | Effect |
| --- | --- | --- |
| `APPLICATION_ID` = `org.obop.mileage` | `app/app.config.ts:24` | Expo `scheme` (:95), Android package (:100), iOS bundle id fallback (:25, :126). A second literal builds the redirect `org.obop.mileage:/oauth` (`src/native/config.ts:6-7`). The two copies are not derived from each other. |
| `OBOP_IOS_BUNDLE_ID` | `app.config.ts:25` | iOS bundle id only. The scheme and the Android package stay `APPLICATION_ID`. The comment at :19-23 gives the reason (Apple signing) and names `org.obop.mileage.test` for test builds. |
| App name, slug | `app.config.ts:91-92` | "obop Mileage". `CLIENT_NAME` repeats it (`config.ts:8`) and goes to the server as `client_name`. |
| Version | `app.config.ts:96,101` | "0.1.0", versionCode 1. Sent as `device_app_version` (`src/native/platform-deps.ts:16-21`, `web/packages/client/src/oauth.ts:352-354`). `package.json` says 0.0.0. |
| `OBOP_DEFAULT_SERVER` | `app.config.ts:40`, then `extra` at :135 | Pre-fills the address field (`src/ui/screens/SignIn.tsx:10-28`). It is a default, not a lock. |
| `OBOP_ALLOW_CLEARTEXT` | `app.config.ts:39` | Android `usesCleartextTraffic` (:56-61) and the run-time address rule (`src/native/auth.ts:48-62`). |
| `OBOP_RECORD_THINNING`, `OBOP_DEV_CLIENT` | `app.config.ts:47`, `:54` | Thinning off is for test builds only and is read again at run time through `extra`; the second keeps the dev-client entries in Info.plist. |
| Language | `src/i18n/index.ts:8` | Fixed `"en"` on `ios`. `Accept-Language` is the literal `"en"` (`auth.ts:93`). On `sv`: `expoLocales()` (`sv:app/app.config.ts:14,95`) and a stored choice (`sv:app/src/native/config.ts:21`). |

There is no setting for a locked server, an icon, a splash, launch colours or per-language iOS permission strings on `ios`. `app.config.ts` has no `icon` or `splash` key (lines 90-172).

What changes when the host owns them (all PROPOSED):

**OAuth registration.** One client for the whole host, class `mobile`. The declared groups are the union of the groups its modules need on a phone.

- At most 32 groups (`obop-commands/src/devices.rs:407`). A group on the never-for-mobile list makes the whole registration fail (`devices.rs:422-426`). Whether a group exists is not checked, so an unknown name still takes a slot (`devices.rs:413-435`).
- The consent page offers only groups that are declared, held and allowed on a phone (`obop-oauth/src/flow.rs:74-85`). The token response carries `scope`, the groups actually handed over (`obop-oauth/src/token.rs:1093-1101`). The host should show a module only when its groups came back.
- The cap bounds the groups that all modules of a host need on a phone together, because the host registers one client. How close an implementation comes to it depends on its own groups (section 9.1 item 2). Two ways to lift it are possible: raise the constant, or a client that an operator registers and the cap does not bind (6.3). Neither is built.
- `mileage.reviewer` and `mileage.admin` end in blocked suffixes (`devices.rs:386-404`; names at `mileage/src/policy.rs:17-19`). Only `mileage.driver` can ride a phone token. A reviewer screen in the host cannot work with a phone token: nothing on main lets a phone token carry such a group.
- `needsRegistration` compares the address and the age of the last use only (`ios:app/src/core/session.ts:27-33`). `ServerConfig` stores no declared groups (:10-17). If a build adds a group to `SCOPE`, an installed app keeps its old client (`needsRegistration` ignores the scope) and sends the new list on the authorize request (`ios:app/src/native/auth.ts:158`). The server refuses a mobile client that asks for a group it did not declare, with a redirect carrying `error=invalid_scope` (`obop-oauth/src/flow.rs:325-331`, `obop-http/src/oauth.rs:406-419`). The app maps that to `SignInError("failed")` (`auth.ts:67-72`) and registers again only when the authorization page answers 400 (`session.ts:44`), so the sign-in fails until the client is registered anew. Removing a group does not fail. Needed: store the declared list with the config and register again when it differs.

**Redirect scheme of the host.** The scheme is `<host id>:/oauth`.

- The server checks the shape only (`obop-oauth/src/redirect.rs:113-134`): a scheme of at least two dot-separated labels (lower-case letters, digits and `-`, each at most 63 characters, no leading or trailing `-`), the scheme starting with a lower-case letter and at most 128 characters; a path that starts with one `/` (not `//`) and holds only letters, digits and `. _ ~ - /`; the whole URI at most 256 characters. https is refused (`redirect.rs:190`).
- A reverse-domain id of an implementation, such as `<reverse-domain id>:/oauth`, has the accepted shape when it meets these rules. The rule was read, not run.
- The server does not reserve the scheme on the phone, so another app can claim it (`main:docs/ISSUES.md:275`, L10).

**Device naming.** The name is the Android brand and model, or "iPhone", "iPad" or "iOS device" on iOS (`ios:app/src/core/device.ts:14-25`), the platform from `Platform.OS`, the version from the Expo config (`platform-deps.ts:16-21`). The server keeps name 80, platform 24 and version 32 characters at most (`obop-commands/src/devices.rs:31-33`), and at most 20 active devices per person (`obop-oauth/src/lib.rs:80`). A host has one version string. The server cannot see module versions. If it must, a field is needed (6.1).

**The standalone ids.**

- `org.obop.mileage` is a placeholder, "not registered anywhere" (`ios:app/app.config.ts:15-17`). `org.obop.mileage.test` is the iOS test bundle id (:19-23).
- PROPOSED: the test build stays a test build and keeps the test id. Whatever obop builds for its own device tests is a reference target, not a build for the stores, and keeps the test id. `org.obop.mileage` stays unused. No build registers it before Q37 is answered.

**What this does to Q37.** Q37 is open and one-way (`main:docs/DECISIONS.md:1621`). D87's limits (PROPOSED on main) say no Apple or Google account is used, no listing is made and nothing is published without the owner (`:1217`). If each implementation builds its own host, the id on a store is that implementation's own, not obop's (not ruled, section 0). That does not answer Q37 for obop's own builds. This document decides nothing about Q37. One effect: an id that has been signed with an Apple team is hard to change (the reason in `app.config.ts:19-23`). PROPOSED: obop registers no id other than the test id until the owner rules.

## 4. Sign-in and the device flow

**The RFC 8252 flow as built** (`ios`):

1. Register. `POST /oauth/register` with class `mobile`, one redirect, and the declared groups as `scope` (`web/packages/client/src/oauth.ts:217-237`; `app/src/native/auth.ts:132-137`). The server reads the class at `obop-oauth/src/client.rs:110-116`.
2. Authorize in the system browser with PKCE S256 (`auth.ts:168`, `oauth.ts:250`). The redirect is parsed by `parseAuthorizationResponse` (`oauth.ts:290`).
3. The person signs in on the server's page and ticks groups. Lifetime choices are 1, 7, 30 and 90 days, default 30 (`obop-oauth/src/flow.rs:223-224`).
4. Exchange the code with device name, platform and app version (`oauth.ts:343-354`). The server creates a `device` record and returns the access token (3600 seconds, `obop-oauth/src/lib.rs:68`), the refresh token, `scope` and `device_id` (`token.rs:1086-1101`).
5. The app calls `device.get` on its own device to learn the owner (`auth.ts:87-101`), binds the outbox to `{origin, user}` (:180-181), and revokes the new sign-in if that fails (:183-192).
6. A refresh retires the previous access token once the rotation has committed; if the store refuses that step it is tried again, and if it is lost the old token ends with its one hour lifetime (`obop-oauth/src/token.rs:741-746`). Presenting a used refresh token is logged as `oauth_refresh_reuse` (`token.rs:576,758`).

**Who owns it in the host (PROPOSED).** The host would own steps 1 to 6 and the sign-in screen. The module receives an authenticated `client`, the device id, the owner and the granted groups. It never sees the refresh token or the Keychain items.

**Sign-out and offboarding, as built:**

- Sign-out starts `revokeSignIn` first, because it needs the refresh token. Then `leaveSession` stops sync, stops an open trip, counts unsent trips, drops the tokens and clears the outbox unless the person kept the unsent trips (`src/native/actions.ts:140-167`).
- "Keep them": the Account screen offers it when trips exist only on the phone. The default is to keep. Deleting needs a second confirmation (`src/ui/screens/Account.tsx:112-138`).
- Foreign trips: the outbox is bound to `{origin, user}`, the canonical server address and the `user` of the device record (`src/native/auth.ts:100`; `src/core/identity.ts:8-19`). A different user or address on the next sign-in gives the `foreign` binding. Nothing is uploaded or shown while it holds (doc comments at `src/native/runtime.ts:21-28`). The gate shows only the Foreign screen (`router.ts:68-83`). The person may sign out and keep the trips (`Foreign.tsx:74`) or delete them and continue (`Foreign.tsx:60`).
- A phone token is valid only while its device record is active. The record is read on every request, and a store that refuses the read means no principal (`obop-auth/src/lib.rs:428-450`).
- In non-test code, `clearAll` has three callers: `identity.ts:88`, `identity.ts:107` and `actions.ts:145`. A server-side end of the sign-in reaches none of them. Unsent trips of a leaver stay on the phone, encrypted, until the next sign-in decides.
- The server ends a person's devices on offboarding: `offboard_as` calls `end_account_tokens` (`obop-auth/src/admin.rs:417`), which calls `end_devices` (`admin.rs:507-509`). It does the same on a credential change (`obop-auth/src/account.rs:92-108`). Verified by reading the calls; nothing was run. A phone is refused at its next call once its device record is no longer active, whichever way the record was ended.

**Host consequences (PROPOSED).** The unsent items are per module. The sign-out dialog would ask every module through the `unsent` hook and pass the choice back through `onSignOut`. The foreign gate would hold all modules, since the owner is one person per sign-in.

**The Microsoft step for phones (PR 47).** It exists on main, against stand-ins only.

- The sign-in page of a mobile app's OAuth flow shows a Microsoft link when the provider is on and the connectors runtime exists. MCP-class flows never see it (`obop-http/src/oauth.rs:447-458`).
- The routes are `GET /oauth/authorize/microsoft` and `GET /oauth/authorize/return` (`obop-http/src/lib.rs:375-376`). The callback is the existing `/api/auth/oidc/callback` (`lib.rs:300`). No second redirect address is registered at Microsoft (`main:docs/DEPLOY.md:384`).
- A phone grant of an account that signs in with Microsoft only lasts at most 30 days. The consent page, the decision and the code exchange all apply it (defined at `obop-oauth/src/flow.rs:227-241`; applied on the consent page at `flow.rs:505-509`, in the decision at `flow.rs:1028-1032` and at the code exchange at `token.rs:327-336`).
- The app needs no Microsoft code. The page is served in the system browser, and the app's registration and redirect do not change. A grep of `ios:app/src` and the client sources finds no Microsoft sign-in code (only generated admin types).
- Not built: a phone step-up; `auth_time` kept on the grant; the Microsoft link for MCP pages; the first-bind rule (item 3b of D91); expiry of agent tokens for Microsoft-bound staff. `reauth_required` is a catalogued error code (`obop-commands/src/errors.rs:106,253-258`, and a list of codes in the tests of the same file at :926); `git grep` finds no other use in `crates`. `security.step_up_secs` has no hit in `crates`.
- Never run against a real tenant, a real phone, `ASWebAuthenticationSession` or Custom Tabs (`main:docs/DEPLOY.md:391`).

**Token limits that matter to a host.**

| Limit | Fact | Cite |
| --- | --- | --- |
| Audience | The token is bound to `<base>/api`. A token that carries an audience fails every route that uses plain `auth`. | `obop-http/src/lib.rs:189-196,215-221` |
| Lifetime | Access token 3600 seconds. Grant 1, 7, 30 or 90 days. | `obop-oauth/src/lib.rs:68`; `flow.rs:223` |
| Accepted routes | `POST /api/{model}/{command}`, `GET /api/meta`, `GET /api/commands`, `GET /api/openapi.json` | `lib.rs:306-308,336,435,537,556,616` |
| Refused: `/mcp` | Audience must be `<base>/mcp`. | `obop-http/src/mcp.rs:455-461` |
| Refused: blobs | Upload and download use plain `auth`. | `obop-http/src/blobs.rs:69,174` |
| Refused: approvals | The route uses plain `auth`, and the dispatcher needs a person. | `lib.rs:647`; `obop-commands/src/lib.rs:1043-1045` |
| Refused: Drive | The handlers of `drive.rs` call `auth`, never `auth_api`. | `obop-http/src/drive.rs:161,222,377` |
| Refused: account routes | `/api/auth/*` stays on plain `auth` (stated in the doc comment; each route not re-read). | `lib.rs:193-196` |
| No approval gate | Approval applies only to the agent principal kind, so a `needs_approval` command runs directly for a phone token. | `obop-commands/src/lib.rs:1015` |

For a host this means a client that uses the host's token cannot upload or download files, approve plans or use account pages. Nothing here is a server change; it limits what a client of the host's token can do.

## 5. Background location and permissions in one app

**What is app-wide today** (`ios`):

- iOS Info.plist: one `NSLocationWhenInUseUsageDescription` naming "obop Mileage" (`app.config.ts:160-161`) and one `NSLocalNetworkUsageDescription` (:131-132). The Always, motion and full-accuracy keys are left out (:156-159).
- iOS background mode: the expo-location option adds `location` (`app.config.ts:150`; the plugin `withLocation.js:115-119` in the installed package). The expo-task-manager plugin adds `fetch` (`withTaskManager.js:24-25`, installed package), which the app's wrapper removes (`app.config.ts:83-88`). Whether the final plist holds exactly `location` is not verified: no `ios/` project is generated and the config test was not run. The two plugin lines were read in the installed packages and were not re-opened for this document.
- Android: foreground service enabled and no background-location permission (`app.config.ts:143-144`). The library declares two services of type `location` (`expo-location/android/src/main/AndroidManifest.xml:9,13`, installed package, not re-opened). The app adds `RECEIVE_BOOT_COMPLETED` (:112) and blocks six permissions (:116-123). The merged manifest is not verified.
- One foreground-service notification, with options set in `recorder.ts:30-42`.
- One task: `LOCATION_TASK` is a single constant (`config.ts:14`) and `defineTask` is called once (`task.ts:22`). Its callback reads the Mileage store and stops updates when no trip is active (`task.ts:34`).
- Prompt order: in-app disclosure, then the foreground location request (`permissions.ts:47`), then the Android notification permission, asked only on Android API level 33 or later (`permissions.ts:74-75`) (`Permissions.tsx:76-77`).

**What that means for a host with other modules:**

1. There is one plist text and one manifest for all modules. A second module that uses location shares the system prompt text and the disclosure flag (`Permissions.tsx:19`). The host needs one permission step whose text names the app and lists purposes, or modules contribute sentences that the host joins.
2. A second module that records needs its own task name. Whether two location tasks run together on both systems is not verified. Nothing in the repository runs two.
3. The iOS background mode and the Android service type are declared once. A module that does not record still ships with them.
4. PROPOSED: the host asks for location the first time a module that needs it is opened, not at launch. This is a design suggestion, not a code fact.

**Store-review exposure**, only what obop's own documents state, no legal advice:

- Google Play allows background location only for core functionality and asks for a Permissions Declaration Form, one background-location feature, a short demonstration video and an in-app disclosure before the runtime prompt (`main:docs/spec/mobile.md:87`). Whether a foreground service started from a visible app avoids the declaration is an open question that the policy text read for that document does not settle (`main:docs/spec/mobile.md:514`). Its risk table lists "Google Play and App Review of background location" (`main:docs/spec/mobile.md:484`).
- Inference, not stated in obop's documents: a store listing covers one binary, so in a host the location declarations, the privacy forms and any reviewer login attach to the host app, not to one module. If each implementation builds its own host, the forms are that implementation's own. Neither point is ruled (section 0).
- obop's advice (PROPOSED): keep the location code, the Android service type, the iOS background mode and the purpose texts in the Mileage module's build, so that a split into a separate binary stays possible. Condition (PROPOSED): no background mode or native module goes into the host before the result of the locked-iPhone test (section 8).
- Whether an iPhone keeps recording with the screen locked under "While Using" is not proven (`ios:docs/ISSUES.md:304`, L42). If the fallback needs "Always" or another API, that change is app-wide.

## 6. What obop's server must provide or change for the host

Each item is PROPOSED. Size classes are a reading of the files touched. They are not measured and carry no time estimate. S: one constant or field with its test and docs. M: several files in one or two crates, client types, tests and docs. L: several crates or a new surface, or an owner ruling is needed first.

**6.1 Version and compatibility handshake. Size M.**

- Today: `core_version` is served (`obop-mcp/src/lib.rs:475`). The app sends `device_app_version` at sign-in (`oauth.ts:352-354`). The server stores it on the device record (`obop-commands/src/devices.rs:211-212`) and puts it into the text of a security-log entry (`devices.rs:439-452`). The token endpoint also accepts `device_app_version` on a refresh and writes it to the device record (`obop-oauth/src/token.rs:554-558,725-733`). `refreshTokens` (`ios:web/packages/client/src/oauth.ts:379-394`) does not send it, so after an app update the record keeps the version of the last sign-in. Nothing compares the stored version with anything. `X-App-Version` is only a name in the CORS allow list (`obop-http/src/cors.rs:33,35`); `git grep` finds no handler that reads it.
- Change: the server states the oldest host version it accepts, and optionally a minimum per module (`ModuleMeta.version` exists, `obop-registry/src/lib.rs:681`). The host reads it at start and shows an update screen below it. A refused-version error code is added.
- Files: `obop-mcp/src/lib.rs`, `obop-commands/src/devices.rs`, `obop-commands/src/errors.rs`, `obop-http/src/lib.rs`, `web/packages/client/src/meta.ts` and the generated schema, `docs/DEPLOY.md`.

**6.2 Icon, categories, version and maturity in `/api/meta`. Size S.**

- Today: `ModuleMeta` has `icon`, `categories`, `version` and `maturity` (`obop-registry/src/lib.rs:679-702`). `meta_json` emits only id, name, models, description, labels, descriptions and summaries (`obop-mcp/src/lib.rs:381-470`).
- The Mileage module sets icon "fleet" and category "people" (`mileage/src/model.rs:21-27`). A grep for a line starting `icon =` over `packages/*.toml` finds 0 of 23 files. The same grep for `categories =` finds 23 of 23.
- Catalogue data is readable only through the `module` model, whose read group is `platform.admin` (`obop-pkg/src/installer.rs:2273-2277`), a never-for-mobile group (`devices.rs:401-402`).
- Change: add the four fields to the module block. Optional and also S to M: a caller block (class, user, device, groups, expiry). Today the groups come back only in the token response, and the user only through `device.get`.
- A manifest table for app-level layout is not proposed here; PROPOSED: defer it until a second consumer needs it. Until then the host's drawer has no server source for its layout.
- Files: `obop-mcp/src/lib.rs:381-470`, `obop-http/src/openapi.rs`, `web/packages/client` types, one test.

**6.3 The declared-groups cap. Size S to raise, M to replace by a rule.**

- Today: 32 (`obop-commands/src/devices.rs:407`). An unknown name takes a slot.
- Change: decide before a second module's groups are declared. Raising the constant touches the PROPOSED design of the D64 mobile amendment, so it needs the owner's word. A rule such as "count only groups that exist" is the M variant.
- The other route is a client that an operator registers and the cap does not bind. It also needs an exemption from client expiry and a place outside the client limit (an unused client is forgotten after a day, an idle one after 180 days, and clients are counted against 2000: `obop-oauth/src/lib.rs:71,78-79`). `main:docs/ISSUES.md:275` (L10) notes that an operator has no setting yet to say which mobile clients may register. That route is not designed and not built.

**6.4 Blob routes for phones. Size M. Needs an owner ruling first (D64 amendment, a sentence in D60).**

- Today: refused (`obop-http/src/blobs.rs:69,174`). `main:docs/DEPLOY.md:366` states the refusal as intended. PR 50 changed the command-layer blob gate (`obop-commands/src/blobs.rs`), not these routes.
- Change: the two handlers accept the `/api` audience (`auth_api`). Decide the cache header: blobs are served with `private, max-age=31536000, immutable` (`blobs.rs:192`), so a WebView or HTTP cache could keep a file after sign-out. PROPOSED: `no-store` for phone principals.
- The Mileage module needs none of this (no files). A module that handles files, for example an attachment, would. Image handling beyond the routes is not proposed here.

**6.5 Idempotency beyond `bulk_create`. No server change in the first release.**

- Today: only `bulk_create` takes `idempotency_key` (`obop-commands/src/engine/bulk.rs:348-352`). It is kept 24 hours, a key reused for different rows is refused, and a person may start 12 calls a minute unless `OBOP_BULK_PER_MINUTE` says otherwise (`bulk.rs:14-18,73-78`). Single `create`, `update` and actions have no key. As a request header, `Idempotency-Key` appears only in the CORS allow list (`cors.rs:30,35`); no handler reads it.
- Mileage works within this: one-row `bulk_create` for the trip and ranged keys for points (`ios:app/src/core/values.ts:98-102`, `src/core/api.ts:10,95`).
- Consequences for a generic outbox: a module needs its own natural key (a client id), and the 12-per-minute limit is shared by every module's drains. A refusal that asks for a fresh sign-in needs a stated rule too (section 1).
- PROPOSED: each module uses one-row `bulk_create` with a unique client id. Revisit if a module needs an idempotent update. Extending the key to `create` would be size M.

**6.6 Registration limits for a first rollout. Size S for settings, M with docs and tests.**

- Today (compiled in, `obop-oauth/src/lib.rs:66-82`): 20 registrations per address per hour, 300 per hour in all, 2000 clients, an unused client forgotten after one day, an idle one after 180 days, 20 devices per person. The two non-test constructions both use `OAuthConfig::default()` (`obop-http/src/lib.rs:86-90` and `crates/obop/src/app.rs:202-206`). `git grep OBOP_OAUTH` finds no override.
- Each phone registers its own client, and registers again when its client is older than 30 days unused (`session.ts:20,33`). A rollout where many phones sign in within one hour from one address meets the per-address limit.
- Change: environment settings for these numbers, read once at start, and a paragraph in `docs/DEPLOY.md`.

**6.7 A naming constraint, not a code change.** Groups that start with `platform.`, end in `.admin`, `_admin`, `.manager`, `.approver`, `_approver`, `.officer`, `.designer`, `.reviewer`, `.publisher`, `.preparer`, `.owner` or `_owner`, or are on the named list can never be declared by a mobile client (`obop-commands/src/devices.rs:316-404`). Two rules follow for a module that declares groups:

- A group that a phone should carry is named outside these patterns.
- A group that approves, reviews or publishes should end in one of the blocked endings, so that the built block keeps it off phone tokens. A group of that kind that ends in none of them is not caught: `devices.rs:312-315` notes that some groups (for example `pos.supervisor`) are on neither list. The boundary is what the client declares and the person ticks.

Which groups go on a phone is each implementation's to name (section 9.1 item 2).

**6.8 Not host needs.** Three things are not proposed here: a way to hand a reviewer-class group to a phone token (sections 3 and 6.7), approving an agent's plan on a phone (section 4) and file handling beyond 6.4. The Swedish phone catalogue is PR 45.

## 7. Build and release facts the host inherits

**Expo SDK and native modules** (exact pins):

| | `main` spike (`app/package.json:19-27,32`) | `ios` and `sv` (`ios:app/package.json:33-52`) |
| --- | --- | --- |
| Expo SDK | 57: expo 57.0.30 | 58: expo 58.0.16 |
| react-native, react | 0.86.3, 19.2.3 | 0.88.0, 19.3.0 |
| expo-location | 57.0.21 | 58.0.18 |
| expo-task-manager | 57.0.21 | 58.0.12 |
| expo-secure-store | 57.0.4 | 58.0.2 |
| expo-sqlite (SQLCipher by config plugin) | 57.0.4 | 58.0.13 |
| expo-dev-client | 57.0.20 | 58.0.14 |
| expo-status-bar | 57.0.1 | 58.0.3 |
| expo-constants, expo-crypto, expo-system-ui, expo-web-browser | not used | 58.0.12, 58.0.7, 58.0.6, 58.0.6 |
| react-native-safe-area-context | not used | 5.10.1 |
| typescript | 6.0.3 | 6.0.3 |

- `expo-system-ui` is declared but imported nowhere in the app (grep of the source). Why it is declared is not shown in code.
- Workspace packages used: `@obop/client`, `@obop/trip-core`, and on the branches `@obop/ui-native`.
- `package.json` lists no navigation, WebView or SVG library.

**Pin policy** (`CLAUDE.md`): pins are exact. "Latest" means the newest release that has passed a cooldown of 7 to 14 days, or its `x.y.1` patch. The toolchain can run up to about three weeks behind. The app starts on SDK 58 if its packages pass the cooldown at the start of the build (OR-34 e, amended). `ios:app/package.json` has `check:sdk` and `check:sdk:latest` scripts (:12, :21). The host inherits one SDK for all modules.

**Build facts:**

- The tree has no `ios/` or `android/` directory (`git ls-tree` of `ios:app/` shows `config`, `scripts`, `src` and the root files only); `expo prebuild` generates them. The device build is `expo run:ios --device --configuration Release` (`ios:app/package.json:20`). iOS builds need a Mac (OR-36).
- No signing configuration is in the app tree or its scripts: `git grep` finds `signingConfig` and `DEVELOPMENT_TEAM` nowhere on `ios`. `ios:docs/ISSUES.md:286` (L24) states that the Android release build is signed with the debug keystore of the generated project.
- CI on `ios` runs `vp check` and `vp test` for `apps/mileage-app` (`.github/workflows/ci.yml:81-82,86-87`). `git grep` finds no gradle, prebuild or xcodebuild line in `.github/workflows`, so CI builds no native code.
- The SQLCipher variant of `expo-sqlite` is a config-plugin option (`app.config.ts:169`) and applies to the whole binary.

**Xcode 27.2 note** (reported in a build report, not reproduced here except where marked):

- Reported: on the Mac mini (Xcode 27.2, Swift 6.4.2) the Release device build of expo-modules-core 58.0.23 crashed swift-frontend (SILGen) at `ios/Core/Functions/AsyncFunctionDefinition.swift` line 200, `case .failure(let error as any Error):`.
- Checked with read-only `gh` calls for this document: the crash is known upstream (expo/expo issue 52610, Xcode 27.2 and Swift 6.4.2, in that file), and expo/expo PR 52631, merged, removes the `as any Error` cast. Reported: the fix shipped in expo-modules-core 58.0.29, and `expo` 58.0.22 and later depend on 58.0.29. `ios:app/package.json:33` pins expo 58.0.16 and `ios:pnpm-lock.yaml:3360` locks expo-modules-core 58.0.23, the version of the reported crash, so the pinned build is on the crashing side. The version numbers were not checked against the registry.
- Reported: that one line was patched in the Mac's `node_modules` copy only. The line is identical to upstream's fix, the original is saved on the Mac, and nothing is committed. A later `vp install` on the Mac without re-applying the patch brings the crash back.
- Reported: with that patch the standalone test build was built from `ios` 89c4e50 and installed on a real iPhone, and the gate run without `--drive` passed. So the test binary is not built from the pinned source alone.
- PROPOSED for the durable fix: bump expo on the app's branch to a release that has passed the pin-policy cooldown (this changes the lockfile and the tested head, so it waits for the owner's word), or a patch recorded in the repository through pnpm's patch support. Any host build on Xcode 27.2 needs one of the two.

## 8. What the standalone iPhone test still proves for a host

The sources are the notes in the repository (`main:docs/ISSUES.md:266,275`; `ios:docs/ISSUES.md:263,304`; `ios:docs/spec/mobile.md:642,644,724`). The test is the existing Mileage app, built from the `ios` branch without a change to its files (the Release build on Xcode 27.2 needed a local patch in `node_modules`, section 7), with obop's own id and redirect and English text, on a real iPhone. Its steps are `ios:docs/spec/mobile.md` 9.2 (line 724). The test build has the bundle id `org.obop.mileage.test` through `OBOP_IOS_BUNDLE_ID`, while the sign-in scheme and redirect stay `org.obop.mileage:/oauth` (`ios:app/app.config.ts:19-25`; 9.2 fixes the bundle id of the test build to `org.obop.mileage.test`). No result of the test is recorded in the repository.

The repository's steps describe a test instance on a PC on the local network, reached over plain http (`OBOP_ALLOW_CLEARTEXT=1`, `OBOP_DEFAULT_SERVER=http://<LAN>:18080`; `ios:docs/spec/mobile.md:724-760`). If the test instance is reached over https instead of plain http on the local network, the steps change as follows:

- The build is made with `OBOP_ALLOW_CLEARTEXT=1`, `OBOP_IOS_BUNDLE_ID=org.obop.mileage.test` and `OBOP_RECORD_THINNING=off`, without `OBOP_DEFAULT_SERVER`. `OBOP_RECORD_THINNING=off` is honoured only where plain http is allowed (`ios:app/app.config.ts:42-46`). A build made with `OBOP_ALLOW_CLEARTEXT=1` accepts any http address, so it is a test build that is given to nobody else (`ios:docs/ISSUES.md:286`, L24).
- The gate is run without `--drive`, because `--drive` fails unless the default server is an `http://` address (`ios:app/scripts/ios/gate-lib.mjs:123-126`) and `--expect-bundle-id` belongs to `--drive` (`ios:app/scripts/ios/gate.mjs:53`).
- The local-network step cannot run when the instance is not on the local network. iOS asks for local network access only when the app reaches an address on the local network (`ios:app/app.config.ts:128-132`).

If the test passes, it proves for the host:

1. Whether an iPhone keeps recording with the screen locked under "While Using" and the `location` background mode (ISSUES L42). That decides whether the host needs "Always" or another API, which would be app-wide.
2. The private-use redirect round trip in a real system browser on a real iPhone. `main:docs/ISSUES.md:266` (L1) says nothing of the mobile sign-in has run with a real app, a phone or a system browser session. `ios:docs/ISSUES.md:263` (L1) adds that the Mileage app's sign-in has run in an Android emulator (Chrome Custom Tab) and in the iOS Simulator (`ASWebAuthenticationSession`), and not on a phone. The host reuses this flow unchanged.
3. The encrypted outbox, the Keychain class and the iOS backup recovery on a real device (`open-outbox.ts:123-127,155-190`, `keychain.ts:13-21`). The host inherits them.
4. Upload from the outbox after offline use, with idempotency keys, against the test instance.
5. A battery reading. `ios:docs/spec/mobile.md` 9.2 (line 814) reports it and does not make it pass or fail; so does acceptance item 5 in section 9 of that file (line 642).

It does not prove:

- Anything about the host: the module seam, routes, bar, drawer, a second module, two location tasks, permission prompts for several modules.
- The id and redirect of any host (for example `<reverse-domain id>:/oauth`). The test registers `org.obop.mileage:/oauth` only (the iOS bundle id of the test build is `org.obop.mileage.test`).
- Microsoft sign-in on a phone, or Conditional Access (nothing has run against a real tenant, `main:docs/DEPLOY.md:391`); translated text (`ios` is English only); store review.
- A clean pinned build. The Release device build on Xcode 27.2 needed a local one-line patch in `node_modules` (section 7, reported), so the test binary is not built from the pinned source alone.

## 9. Open questions

### 9.1 For an implementation that builds the host

1. Where the data files of an implementation live (the registry data of its modules, the native screens, the build) is not ruled (section 0). obop publishes nothing to the npm registry until the owner rules at W2 (OR-19, Q32; the default is no publication, `CLAUDE.md`; the default channels are GitHub releases and crates.io, `main:docs/DECISIONS.md:1616`), so the working assumption (PROPOSED) is a repository of the implementation's own that pins an obop checkout.
2. Which groups will your modules declare on a phone, and does any of them approve, review or publish? Name each one that does and ends in none of the blocked endings (6.7).
3. Do you need the version handshake (6.1) before the first internal build?
4. Do you have a requirement on the host's database layout (one file or one file per module) or on the sign-out dialog (section 1)?
5. Whether an implementation plans a native screen of its own that records location in the first release decides whether two location tasks must be tested (section 5).
6. Will more than 20 phones sign in for the first time within one hour from one address (6.6)?
7. Which Xcode version will the Mac that builds your iOS binary run (a version)? Section 7 shows why it matters.

### 9.2 For the owner (only the owner can answer these)

1. Does the standalone Mileage app stay buildable as its own binary after the host exists?
2. May D87's first-party limit be reworded around what the project publishes under its own name (change code W of section 10)?
3. Is the phone host outside the UI-parity rule (OR-16, D80), with a written reason for the exemption list? `CLAUDE.md` says nothing is exempt until the owner approves it.
4. Does obop register any store id of its own before Q37 is answered? Q37 itself stays the owner's.

## 10. Documents on main that this direction makes stale

State after the follow-up change of 2026-11-09 (branch `docs/host-direction`, based on main 9ba3afed). The codes H, M and P are applied in the rows that carry them. Code W is applied nowhere: the wording of D87's first-party limit is the owner's and is PROPOSED (Q63, question O2), and no ratified row is rewritten. A row whose only code is W is not touched, and in a row that carries W with other codes the W part waits. All 23 rows were found again by their text on main 9ba3afed; none had changed or gone. Six had moved, all in `docs/DECISIONS.md`: the Line column is that of main at fe1b93a, and the last column gives the new line where it differs. H is the one wording of section 0. M and P are done as their codes say, with one choice: "a module of the host" is added beside the words "the app" or "the mobile app", in the form "in the planned host design a module of the host", and does not replace them, because the standalone app is still what D87 and `docs/spec/mobile.md` specify. In the table below the third column is the text on main 9ba3afed before the change (a row that is applied now reads as the edit made it), and "applied" and "waits" refer to that change.

Why none of them is edited by this document:

- PR 27, 34 and 45 are stacked and edit the same lines. The old-line starts of `git diff -U0` between the merge-base 36fa622 and `feat/mileage-sv` are merge-base line numbers, not main's. By content, `DECISIONS.md` lines 227 and 239 are the same on main fe1b93a, and merge-base lines 1197 and 1217 are 1199 and 1219 on main; the other hunks are not mapped. The hunks are `CLAUDE.md` 42; `docs/DECISIONS.md` 89, 130, 135, 227, 239, 1197, 1217, 1617, 1622; `docs/DEPLOY.md` 361, 364, 384; `docs/ROADMAP.md` 187, 379; `docs/ENGINEERING.md` 180, 186; `docs/ISSUES.md` 259, 263, 281 (these two files are not in the table below); `docs/spec/mobile.md` in many places (28 hunks).
- PR 51 edits some of the same files, but not the lines below. Its old-line starts, against its merge-base cff7076, are `CLAUDE.md:53`, `DECISIONS.md` 231, 590, 1280, 1387, 1389, 1581, 1646, `GLOSSARY.md:116`, `MODULES.md:33`, `PRODUCT.md:741`, `ROADMAP.md:520`, `ux.md:663`, `services.md:665`, `CONVENTIONS.md:52,246`. It also edits `ISSUES.md`, `SIGN-DOCUSEAL.md`, `spec/finance.md`, `spec/integrations.md` and `spec/security-privacy.md`, and adds `SIGN.md`; none of these is in the table. Line numbers shift on merge, so find each row again by its text.
- The wording of D87's limit awaits the owner (`main:docs/DECISIONS.md:241`), and the direction was not recorded on main when this document was written (section 0).

Merge order that avoids conflicts, each merge on the owner's word:

1. PR 27, then 34, then 45 (stacked). 2. PR 51 (independent of the lines below; merged since, 09861505). 3. One docs-only PR from the new main with every row below, plus the entry for this file in the README document map and the entries OR-39, D93 and Q63 of section 0. Step 3 is the follow-up change of 2026-11-09, with the difference that the rows marked "waits" below are left for the owner; at that date PR 27, 34 and 45 are still not merged (none is an ancestor of main).

Codes for the change column:

- W: reword the limit so that it concerns what SpaceCorps publishes under the obop name (proposed in section 9.2 item 2; no ruling).
- H: add one sentence that the phone client is one host app that holds modules, Mileage the first.
- M: say "a module of the host" where the text says "the app".
- P: drop "planned at `web/apps/mileage-app` with `packages/mileage.toml`". The same bullet already says there is no `packages/mileage.toml`.

| File | Line | Current text (cut to 160 characters) | Change | State on main 9ba3afed |
| --- | --- | --- | --- | --- |
| `CLAUDE.md` | 42 | the proposal limits them to first-party apps and to cases a PWA cannot serve (D87, Proposed). Expo SDK 57 for now; the app starts on SDK 58 if its packages | W, H, P | H and P applied; W waits |
| `docs/ARCHITECTURE.md` | 809 | The proposal limits them to first-party apps and to cases a PWA cannot serve (recording a trip while the phone is locked is the first case) | W, H | H applied; W waits |
| `docs/ARCHITECTURE.md` | 929 | mileage-app/        # React Native (Expo) app, the first native app (D87, Proposed; spec/mobile.md) | M | M applied |
| `docs/ARCHITECTURE.md` | 933 | client  tokens  trip-core                # D87, Proposed: shared by the web and the mobile app (spec/mobile.md 8.1) | M | M applied |
| `docs/DECISIONS.md` | 89 | D87 proposes to limit this to first-party apps and to cases a PWA cannot serve; that limit came with the proposal, is no | W (note, not a rewrite of the ratified row) | Waits (W): a ratified row, not touched |
| `docs/DECISIONS.md` | 227 | the proposal limits them to first-party apps and to cases a PWA cannot serve (Proposed), keeps the PWA as the default and starts with Mileage | W, H | H applied; W waits |
| `docs/DECISIONS.md` | 241 | Proposed and awaiting the owner: the scope limits of D87 (first-party apps only, only where a PWA cannot do the job, the PWA stays the default | W | Waits (W): not touched; now line 244 |
| `docs/DECISIONS.md` | 1199 | The scope limits that came with the proposal (first-party apps only, only where a PWA cannot do the job, the PWA stays the default, Tauri mobile | W | Waits (W): not touched; now line 1206 |
| `docs/DECISIONS.md` | 1201 | The proposal limits them to first-party apps and to cases where a PWA cannot do the job, that is where a background capability is needed | W, H | H applied; W waits; now line 1208 |
| `docs/DECISIONS.md` | 1221 | A mileage report needs the route while the phone is in a pocket, which a PWA cannot promise (general knowledge), and a native client is the smallest th | H (the reason now includes the host app) | H applied; now line 1228 |
| `docs/DECISIONS.md` | 334 | A native app (D87) keeps its queued writes in a SQLite database on the device | M (the outbox contract is per module) | M applied; now line 337 |
| `docs/DECISIONS.md` | 1541 | the limits to first-party apps and to cases a PWA cannot serve are Proposed (D87). Tauri mobile is still not pursued (Proposed), and the PWA stays | W | Waits (W): not touched; now line 1567 |
| `docs/GLOSSARY.md` | 194 | the proposal limits them to first-party apps and to cases a PWA cannot serve (D87, Proposed). The first is Mileage | W, H | H applied; W waits |
| `docs/PRODUCT.md` | 380 | *Not planned*: a suite of native apps; the PWA stays the default client. *Ships* (Proposed, D87): one React Native (Expo) app, Mileage, which w | H | H applied |
| `docs/PRODUCT.md` | 616 | (Proposed, D87: first-party apps only, and only where a PWA cannot serve; the first is Mileage | W | Waits (W): not touched |
| `docs/ROADMAP.md` | 187 | The first native slice, "one background-recorded trip from a real phone" ([spec/mobile.md](mobile.md) 9), is client work beside the waves | M | M applied |
| `docs/spec/mobile.md` | 13 | D87 proposes to limit them to first-party apps and to cases a PWA cannot serve (Proposed), and names Mileage as the first. This document specifies that app | W, H, M | H and M applied; W waits |
| `docs/spec/mobile.md` | 22 | The scope limits: first-party apps only, only where a PWA cannot do the job; the PWA stays the default; Tauri mobile is not pursued | W | Waits (W): not touched |
| `docs/spec/mobile.md` | 259 | web/apps/mileage-app/     the Expo app (development build, config plugins, no checked-in ios/ or android/) | M (and a pointer to this file) | M applied, with the pointer to this file |
| `docs/spec/mobile.md` | 469 | obop does not plan a suite of native apps: the PWA stays the default client, and D87 proposes that a native app exists only where a PWA cannot do the job | W, H | H applied; W waits |
| `docs/spec/people.md` | 351 | the proposal limits them to cases a PWA cannot serve (Proposed, D87, [mobile.md](mobile.md)); the first is Mileage, which is not a self-service page | W, H | H applied; W waits |
| `docs/spec/people.md` | 590 | the limits to first-party apps and to cases a PWA cannot serve are Proposed, D87); employee self-service stays a PWA baseline | W | Waits (W): not touched |
| `docs/spec/ux.md` | 629 | Proposed (D87): first-party apps only, only where a PWA cannot serve, the PWA stays the default client | W | Waits (W): not touched |

Not stale by this direction, and so not in the table: `docs/spec/mobile.md:439` (Android first; stale by OR-36), `docs/DEPLOY.md:361` and `docs/DECISIONS.md:725` (the example scheme `org.obop.mileage:/oauth` is only an example, the server does not require it), and the licence uses of "first-party app" in `BUSINESS.md`, `PRODUCT.md:148,535` and `DECISIONS.md:248,639,1502`.

## 11. Not verified

- Nothing was built, run or tested. Every "verified" mark means the line was opened on its ref. No phone, emulator, WebView, Microsoft tenant, instance or server was used.
- The merged Android manifest and the final iOS Info.plist. No `ios/` or `android/` project exists. Plugin behaviour (`withLocation.js`, `withTaskManager.js`, the `expo-location` manifest) was read from the installed `expo-location` 58.0.18 and `expo-task-manager` 58.0.12 sources, outside the repository, and those lines were not re-opened for this document. Whether the app wrapper leaves exactly `location` in `UIBackgroundModes` is a claim of a code comment and a test that were not run.
- How many of the 32 declared-group slots the groups of the built-in modules and the bundled packages already take. No count was made for this document, so section 3 gives none.
- Whether two expo-location tasks can record at once on Android and iOS. Nothing in the repository tries it.
- Whether a refresh extends a grant (an earlier pass found a fixed expiry; it was not traced).
- The Android branch was not read separately (PR 27 is inside `ios`). `sv` was read through its file list, `sv:app/src/i18n/languages.ts`, the `app.config.ts` lines named above and the config diff; its tests and the completeness of the Swedish text were not checked.
- `web/packages/ui-native` (theme tokens) was not re-read. Each route under `/api/auth/*` was not re-read; the refusal rests on the doc comment at `obop-http/src/lib.rs:193-196` and on `main:docs/DEPLOY.md:366`.
- The Xcode 27.2 report: the line number 200 and the version numbers 58.0.23, 58.0.29 and 58.0.22, the local patch, and the build and gate result on the iPhone. Only the upstream issue and PR were checked, and the lockfile line that locks 58.0.23. The direction of section 0 was not recorded on main when this document was written.
- Doc line numbers are for main at fe1b93a and will move when PR 27, 34, 45 and 51 merge.
