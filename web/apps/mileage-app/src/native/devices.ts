import { ObopApiError } from "@obop/client";
import { withDeadline } from "../core/deadline.ts";
import { leaveSession } from "./actions.ts";
import type { Runtime } from "./runtime.ts";

/** This phone as the instance lists it. A mobile token sees its own device and no other (DEPLOY.md "Mobile apps"). */
export interface DeviceView {
  id: string;
  name: string;
  platform: string;
  appVersion: string;
  lastSeen: string | null;
  state: string;
}

interface DeviceValue {
  id?: string;
  name?: string;
  platform?: string;
  app_version?: string;
  last_seen?: string;
  state?: string;
}

/**
 * How long a call about this phone waits for the instance. React Native's network stack has no
 * timeout of its own, and a stalled call would leave the Account screen waiting for minutes.
 */
const DEVICE_CALL_MS = 30_000;

function deviceId(rt: Runtime): string {
  const id = rt.session.deviceId();
  if (!id) {
    throw new ObopApiError({
      status: 404,
      code: "not_found",
      message: "This sign-in has no device",
    });
  }
  return id;
}

export async function readDevice(rt: Runtime): Promise<DeviceView> {
  const id = deviceId(rt);
  const { value } = await withDeadline(DEVICE_CALL_MS, (signal) =>
    rt.client().call<DeviceValue>("device", "get", { id }, { signal }),
  );
  return {
    id,
    name: value.name ?? "",
    platform: value.platform ?? "",
    appVersion: value.app_version ?? "",
    lastSeen: value.last_seen ?? null,
    state: value.state ?? "",
  };
}

export async function renameDevice(rt: Runtime, name: string): Promise<void> {
  const id = deviceId(rt);
  await withDeadline(DEVICE_CALL_MS, (signal) =>
    rt.client().call("device", "update", { id, name: name.trim() }, { signal }),
  );
}

/**
 * "Remove this phone": the instance ends the sign-in and marks the device revoked, and only when it
 * has answered does the app sign out as in `signOut` (`keepUnsent` decides about the trips that are
 * not completely on the server). If the instance cannot be reached, or does not answer within
 * 30 seconds, the call throws and nothing is removed on the phone, so the phone is never left
 * listed as active on the instance while the app thinks it is gone. (After a timeout the instance
 * may have revoked the device nevertheless: the next call then finds the sign-in ended, and the
 * app signs out by the ordinary path.) Unlike "Sign out", this does not wait only five seconds
 * and then go on without an answer: the driver asked for the instance to be told.
 */
export async function removeThisDevice(rt: Runtime, keepUnsent: boolean): Promise<void> {
  const id = deviceId(rt);
  await withDeadline(DEVICE_CALL_MS, (signal) =>
    rt.client().call("device", "revoke", { id }, { signal }),
  );
  await leaveSession(rt, keepUnsent);
}
