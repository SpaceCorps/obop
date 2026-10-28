import { Banner, Button, Field, Row, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import {
  canDiscard,
  canSubmit,
  formatDistance,
  integrityLines,
  statusText,
  tripStatus,
} from "../../core/present.ts";
import { formatDateTime, t, type MessageKey } from "../../i18n/index.ts";
import { discardTrip, retryTrip, submitTrip } from "../../native/actions.ts";
import type { Runtime } from "../../native/runtime.ts";
import { useOutbox } from "../hooks.ts";
import type { Nav } from "../router.ts";

const when = (iso: string | null): string | undefined =>
  iso === null ? undefined : formatDateTime(iso);

/** The sentence for a refusal, by its code; the server's own words go on a separate line. */
const problemText = (code: string): string => {
  switch (code) {
    case "state_conflict":
    case "discarded":
    case "gone":
    case "unknown":
      return t(`trip.problem.${code}`);
    case "unexpected_answer":
      return t("trip.problem.invalid_response");
    default:
      return t("trip.problem.generic");
  }
};

export function TripDetail({ rt, nav, id }: { rt: Runtime; nav: Nav; id: string }) {
  const { trips, loaded } = useOutbox(rt);
  const trip = trips.find((x) => x.clientTripId === id);
  const [discarding, setDiscarding] = useState(false);
  const [reason, setReason] = useState("");
  const [reasonError, setReasonError] = useState<string | null>(null);
  const [actionFailed, setActionFailed] = useState(false);

  /** Runs one of the driver's actions; a rejection is shown, not lost. */
  const act = (work: () => Promise<void>, after?: () => void) => {
    setActionFailed(false);
    work().then(after, () => setActionFailed(true));
  };

  // Read the server's view of the trip once when it is opened.
  useEffect(() => {
    rt.engine.refreshTrip(id).catch(() => undefined);
  }, [rt, id]);

  if (!trip) {
    return (
      <Screen title={t("trip.title")} onBack={nav.back}>
        {loaded ? <Text tone="muted">{t("trip.notfound")}</Text> : null}
      </Screen>
    );
  }

  const status = tripStatus(trip);
  const distance = formatDistance(trip.distanceM);

  const confirmDiscard = () => {
    if (reason.trim() === "") {
      setReasonError(t("discard.reason.helper"));
      return;
    }
    act(
      () => discardTrip(trip.clientTripId, reason),
      () => {
        setDiscarding(false);
        nav.back();
      },
    );
  };

  const footer = discarding ? (
    <>
      <Button
        label={t("discard.confirm")}
        variant="danger"
        onPress={confirmDiscard}
        testID="discard-confirm"
      />
      <Button
        label={t("common.cancel")}
        variant="ghost"
        onPress={() => {
          setDiscarding(false);
          setReasonError(null);
        }}
      />
    </>
  ) : (
    <>
      {canSubmit(trip) ? (
        <Button
          label={t("trip.submit")}
          hint={t("trip.submit.hint")}
          onPress={() => act(() => submitTrip(trip.clientTripId))}
          testID="submit"
        />
      ) : null}
      {canDiscard(trip) ? (
        <Button
          label={t("trip.discard")}
          variant={canSubmit(trip) ? "secondary" : "danger"}
          onPress={() => setDiscarding(true)}
          testID="discard"
        />
      ) : null}
    </>
  );

  return (
    <Screen title={t("trip.title")} onBack={nav.back} footer={footer}>
      {actionFailed ? (
        <Banner tone="danger" title={t("trip.action.failed")} testID="action-failed" />
      ) : null}
      {trip.problem && trip.problem.code !== "incomplete" ? (
        <>
          <Banner
            tone="danger"
            title={t("trip.problem.title")}
            message={`${problemText(trip.problem.code)} ${t(
              trip.problem.code === "unknown" ? "trip.problem.hint.unknown" : "trip.problem.hint",
            )}`}
            action={{
              label: t("common.retry"),
              onPress: () => act(() => retryTrip(trip.clientTripId)),
            }}
            testID="problem"
          />
          <Text tone="muted" testID="problem-details">
            {t("trip.problem.details", { message: trip.problem.message })}
          </Text>
        </>
      ) : null}
      {trip.problem && trip.problem.code === "incomplete" ? (
        <Banner
          tone="warning"
          title={t("trip.incomplete.title")}
          message={t("trip.incomplete.message")}
          testID="incomplete"
        />
      ) : null}
      {status === "rejected" && trip.rejectReason ? (
        <Banner tone="warning" title={t("trip.rejected", { reason: trip.rejectReason })} />
      ) : null}
      {status === "waiting" || status === "uploading" ? (
        <Banner tone="info" title={t("trip.waiting")} />
      ) : null}

      {discarding ? (
        <>
          <Text variant="heading">{t("discard.title")}</Text>
          <Text>{t("discard.message")}</Text>
          <Field
            label={t("discard.reason")}
            value={reason}
            onChangeText={(text) => {
              setReason(text);
              setReasonError(null);
            }}
            helper={t("discard.reason.helper")}
            {...(reasonError === null ? {} : { error: reasonError })}
            multiline
            testID="discard-reason"
          />
        </>
      ) : (
        <>
          <Row title={t("trip.state")} value={statusText(status)} testID="status" />
          <Row
            title={t("trip.distance")}
            value={distance ?? t("trip.distance.pending")}
            testID="distance"
          />
          <Row title={t("trip.started")} value={when(trip.startedAt) ?? ""} />
          {trip.endedAt ? <Row title={t("trip.ended")} value={when(trip.endedAt) ?? ""} /> : null}
          <Row title={t("trip.purpose")} value={t(`purpose.${trip.purpose}` as MessageKey)} />
          {trip.integrity ? (
            <Row title={t("trip.checks")} subtitle={integrityLines(trip.integrity).join("\n")} />
          ) : null}
        </>
      )}
    </Screen>
  );
}
