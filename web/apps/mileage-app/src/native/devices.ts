import { ObopApiError } from "@obop/client";
import { emitChange } from "./bus.ts";
import { stopRecording } from "./recorder.ts";
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
  const { value } = await rt.client().call<DeviceValue>("device", "get", { id });
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
  await rt.client().call("device", "update", { id: deviceId(rt), name: name.trim() });
}

/**
 * "Remove this phone": the instance ends the sign-in and marks the device revoked; then the tokens
 * and every trip on the phone go, as in a sign-out. If the instance cannot be reached the
 * call throws and nothing is removed.
 */
export async function removeThisDevice(rt: Runtime): Promise<void> {
  await rt.client().call("device", "revoke", { id: deviceId(rt) });
  rt.sync.stop();
  try {
    await stopRecording();
  } catch {
    // Nothing was running.
  }
  await rt.session.signOut();
  await rt.store.clearAll();
  emitChange();
}
