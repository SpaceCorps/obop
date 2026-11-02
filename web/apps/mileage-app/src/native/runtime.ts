import { ObopApiError, createObopClient, revokeToken, type ObopClient } from "@obop/client";
import { createTripApi, type TripApi } from "../core/api.ts";
import { createEngine, type Engine } from "../core/engine.ts";
import { bindOwner, bindingOf, takeOver, type Binding, type Owner } from "../core/identity.ts";
import { createSession, type Session } from "../core/session.ts";
import type { OutboxStore } from "../core/store.ts";
import { createSyncController, type SyncController } from "../core/sync.ts";
import { emitChange } from "./bus.ts";
import { oauthDeps } from "./platform-deps.ts";
import { secureStorage } from "./secure-storage.ts";
import { getStore } from "./store.ts";

/** Everything the screens use, built once. */
export interface Runtime {
  store: OutboxStore;
  session: Session;
  engine: Engine;
  sync: SyncController;
  /**
   * A typed client of the instance the phone is signed in to, or throws a 401 when it is none or
   * when the trips on the phone belong to somebody else (nothing is sent for them).
   */
  client(): ObopClient;
  /** Tells the instance to end this sign-in. Best effort, and bounded: false when it could not be reached in time. */
  revokeSignIn(): Promise<boolean>;
  /** Whether the trips in the outbox belong to this sign-in. Nothing is uploaded or shown while they do not. */
  binding(): Binding;
  /** A sign-in has finished as `owner`: decide about the trips on the phone before anything is uploaded. */
  bindIdentity(owner: Owner): Promise<Binding>;
  /** The driver chose to delete the other sign-in's trips and go on. */
  takeOverOutbox(): Promise<void>;
  /**
   * The instance could not be told that this phone is signed out: the last sign-out, or a sign-in
   * the phone ended on its own after a refresh without an answer (shown on the sign-in screen
   * while it is open).
   */
  signOutUntold(): boolean;
  /** Sets the flag and tells the screens (`emitChange`), so that an open sign-in screen follows it. */
  setSignOutUntold(untold: boolean): void;
}

/** How long a sign-out waits for the instance before it removes the tokens anyway. */
const REVOKE_TIMEOUT_MS = 5000;

const notSignedIn = () =>
  new ObopApiError({ status: 401, code: "not_signed_in", message: "Not signed in" });

async function build(): Promise<Runtime> {
  const store = await getStore();
  let untold = false;
  const setUntold = (value: boolean) => {
    if (untold === value) return;
    untold = value;
    emitChange();
  };
  const session = createSession({
    storage: secureStorage,
    oauth: oauthDeps,
    // A sign-in the phone ended on its own and could not tell the instance about: the sign-in
    // screen says so, as after a sign-out that could not reach the instance.
    onEndUntold: () => setUntold(true),
  });
  await session.restore();
  let binding: Binding =
    session.status() === "signed_in" ? await bindingOf(store, session.config()) : { kind: "own" };

  let cached: { baseUrl: string; client: ObopClient } | null = null;
  const client = (): ObopClient => {
    const config = session.config();
    if (!config || session.status() !== "signed_in") throw notSignedIn();
    // Not even a call that is on its way when the outbox turns out to belong to someone else.
    if (binding.kind !== "own") throw notSignedIn();
    if (cached?.baseUrl !== config.baseUrl) {
      cached = {
        baseUrl: config.baseUrl,
        client: createObopClient({
          baseUrl: config.baseUrl,
          getToken: () => session.accessToken(),
          getLanguage: () => "en",
        }),
      };
    }
    return cached.client;
  };

  // The api follows the session: a call made after the sign-in changed uses the new client.
  const api: TripApi = {
    createTrip: (row, key) => createTripApi(client()).createTrip(row, key),
    findTrip: (id) => createTripApi(client()).findTrip(id),
    getTrip: (id) => createTripApi(client()).getTrip(id),
    listTrips: (limit) => createTripApi(client()).listTrips(limit),
    createPoints: (rows, key) => createTripApi(client()).createPoints(rows, key),
    updateTrip: (id, fields) => createTripApi(client()).updateTrip(id, fields),
    endTrip: (id) => createTripApi(client()).endTrip(id),
    submitTrip: (id) => createTripApi(client()).submitTrip(id),
    discardTrip: (id) => createTripApi(client()).discardTrip(id),
  };

  // A pass that is running when the sign-in changes (sign-out, a new sign-in) stops before its
  // next call, so that nothing of the old sign-in's outbox is sent with the new sign-in's token.
  const engine = createEngine({ store, api, identity: () => session.generation() });
  const sync = createSyncController({
    engine,
    // Nothing is uploaded while the trips on the phone belong to somebody else.
    signedIn: () => session.status() === "signed_in" && binding.kind === "own",
    onRejected: () => session.rejected(),
  });
  sync.subscribe(emitChange);
  session.subscribe(() => {
    emitChange();
    if (session.status() === "signed_in") sync.kick();
  });

  return {
    store,
    session,
    engine,
    sync,
    client,
    async revokeSignIn() {
      const config = session.config();
      const token = session.refreshToken();
      if (!config || !token) return true;
      try {
        await revokeToken(oauthDeps, {
          baseUrl: config.baseUrl,
          clientId: config.clientId,
          token,
          timeoutMs: REVOKE_TIMEOUT_MS,
        });
        return true;
      } catch {
        return false;
      }
    },
    binding: () => binding,
    async bindIdentity(owner) {
      binding = await bindOwner(store, owner);
      emitChange();
      return binding;
    },
    async takeOverOutbox() {
      const config = session.config();
      if (!config || config.user === undefined) return;
      await takeOver(store, { origin: config.baseUrl, user: config.user });
      binding = { kind: "own" };
      emitChange();
      sync.kick();
    },
    signOutUntold: () => untold,
    setSignOutUntold: setUntold,
  };
}

let runtime: Promise<Runtime> | null = null;

/** The runtime of this process, built on first use (opens the database, reads the session). */
export function getRuntime(): Promise<Runtime> {
  runtime ??= build()
    .then((rt) => {
      if (rt.session.status() === "signed_in") rt.sync.kick();
      return rt;
    })
    .catch((error: unknown) => {
      runtime = null;
      throw error;
    });
  return runtime;
}
