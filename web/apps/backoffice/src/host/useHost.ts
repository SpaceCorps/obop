// The host as a component sees it: whether the page is inside the phone host app, what the host
// said about itself, and what the sign-in page may offer there. The component is drawn again when
// the host's answer arrives.
import { useMemo, useSyncExternalStore } from "react";
import type { HostInfo } from "@obop/host-bridge";
import { allowsIn, hostState, subscribe, type HostFeature } from "./state.ts";

export type Host = {
  embedded: boolean;
  info: HostInfo | null;
  /** Always true in a browser; inside the host, what `host.info` says, and false until it has. */
  allows: (feature: HostFeature) => boolean;
};

export function useHost(): Host {
  const state = useSyncExternalStore(subscribe, hostState, hostState);
  return useMemo(
    () => ({
      embedded: state.embedded,
      info: state.info,
      allows: (feature) => allowsIn(state, feature),
    }),
    [state],
  );
}
