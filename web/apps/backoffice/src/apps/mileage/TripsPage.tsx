// Trips in the back office: the generic page, opened on the trips to review, without New and Import
// (trips come from the Mileage phone app; `mileage_trip.create` stays open to it and to agents).
import { useEffect, useMemo } from "react";
import type { ModelPageProps } from "../../extensions.ts";
import { ModelPage } from "../../ModelPage.tsx";
import { encodeState } from "../../query.ts";

/** The first time Mileage opens in a session it lands on "To review"; later visits keep their search. */
let landed = false;

export const TO_REVIEW = encodeState({
  facets: [{ op: "is", f: "state", v: ["in_review"] }],
  group: [],
});

export function TripsPage(p: ModelPageProps) {
  const { route, go, model } = p;
  useEffect(() => {
    if (landed) return;
    landed = true;
    if (!route.st && !route.id && !route.view)
      go({ app: route.app, model: model.name, st: TO_REVIEW }, { replace: true });
  }, [route, go, model.name]);
  const shown = useMemo(() => ({ ...model, can_create: false, can_bulk_create: false }), [model]);
  return <ModelPage {...p} model={shown} />;
}
