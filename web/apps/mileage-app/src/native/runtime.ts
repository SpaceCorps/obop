import { ObopApiError, createObopClient, revokeToken, type ObopClient } from "@obop/client";
import { createTripApi, type TripApi } from "../core/api.ts";
import { createEngine, type Engine } from "../core/engine.ts";
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
  /** A typed client of the instance the phone is signed in to, or throws a 401 when it is none. */
  client(): ObopClient;
  /** Tells the instance to end this sign-in. Best effort: false when it could not be reached. */
  revokeSignIn(): Promise<boolean>;
}

const notSignedIn = () =>
  new ObopApiError({ status: 401, code: "not_signed_in", message: "Not signed in" });

async function build(): Promise<Runtime> {
  const store = await getStore();
  const session = createSession({ storage: secureStorage, oauth: oauthDeps });
  await session.restore();

  let cached: { baseUrl: string; client: ObopClient } | null = null;
  const client = (): ObopClient => {
    const config = session.config();
    if (!config || session.status() !== "signed_in") throw notSignedIn();
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

  const engine = createEngine({ store, api });
  const sync = createSyncController({
    engine,
    signedIn: () => session.status() === "signed_in",
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
        });
        return true;
      } catch {
        return false;
      }
    },
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
