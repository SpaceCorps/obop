// Mileage: the trip list opens on the trips to review, and a trip shows its route, its figures and
// the server's check of the track above the form; Confirm and Reject stand side by side in the
// action row, Reject asking for the reason (UI plan 3.6). Submit is drawn by the app too, so that a
// private trip, which is never reviewed, has none.
import { registerModelPage, registerRecordExtension } from "../extensions.ts";
import { REJECT, RejectAction } from "../apps/mileage/RejectAction.tsx";
import { SUBMIT, SubmitAction } from "../apps/mileage/SubmitAction.tsx";
import { TripReview } from "../apps/mileage/TripReview.tsx";
import { TripsPage } from "../apps/mileage/TripsPage.tsx";

const TRIP = "mileage_trip";

registerModelPage(TRIP, TripsPage);
registerRecordExtension(TRIP, {
  actions: [SubmitAction, RejectAction],
  panels: [TripReview],
  overrides: [REJECT, SUBMIT],
});
