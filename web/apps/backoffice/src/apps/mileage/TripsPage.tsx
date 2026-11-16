// Trips in the back office: the generic page, opened on the trips to review, without New and Import
// (trips come from the Mileage phone app; `mileage_trip.create` stays open to it and to agents). An
// empty review queue is the good outcome of the day, so it says so instead of "No matches".
import { useCallback, useEffect, useMemo } from "react";
import { t } from "@obop/ui";
import type { ModelPageProps } from "../../extensions.ts";
import { ModelPage } from "../../ModelPage.tsx";
import { emptyState, encodeState, type ViewState } from "../../query.ts";
import { EmptyState } from "../../ui.tsx";

/** The first time Mileage opens in a session it lands on "To review"; later visits keep their search. */
let landed = false;

export const TO_REVIEW = encodeState({
  facets: [{ op: "is", f: "state", v: ["in_review"] }],
  group: [],
});

/** Whether a search is exactly "the trips in review" (the queue), whatever it is grouped by. */
export function isQueue(s: ViewState): boolean {
  const [only, ...rest] = s.facets;
  return (
    !rest.length &&
    only?.op === "is" &&
    only.f === "state" &&
    only.v.length === 1 &&
    only.v[0] === "in_review"
  );
}

export function TripsPage(p: ModelPageProps) {
  const { route, go, model } = p;
  useEffect(() => {
    if (landed) return;
    landed = true;
    if (!route.st && !route.id && !route.view)
      go({ app: route.app, model: model.name, st: TO_REVIEW }, { replace: true });
  }, [route, go, model.name]);
  const shown = useMemo(() => ({ ...model, can_create: false, can_bulk_create: false }), [model]);
  const noMatch = useCallback(
    (s: ViewState) =>
      isQueue(s) ? (
        <EmptyState
          icon="check-circle"
          title={t("Nothing to review")}
          hint={t("Every trip sent to review has been confirmed or rejected.")}
          secondary={{
            label: t("Show all trips"),
            onClick: () => go({ app: route.app, model: model.name, st: encodeState(emptyState()) }),
          }}
        />
      ) : null,
    [go, route.app, model.name],
  );
  return <ModelPage {...p} model={shown} noMatch={noMatch} />;
}
