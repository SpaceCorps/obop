import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import type { Purpose } from "../../core/types.ts";
import { t } from "../../i18n/index.ts";
import { startTrip } from "../../native/actions.ts";
import {
  locationAccess,
  openSettings,
  requestLocation,
  requestNotifications,
  type LocationAccess,
} from "../../native/permissions.ts";
import type { Runtime } from "../../native/runtime.ts";
import type { Nav } from "../router.ts";

/** The key under which the outbox remembers that the driver has read the disclosure. */
export const DISCLOSURE_META = "location_disclosure_accepted";

type Step = "check" | "disclosure" | "denied" | "blocked" | "guide";

/**
 * Before the first Start: the disclosure in the app, in plain words, before the system dialog
 * (Google Play asks for it when location is collected in the background); then the permission; then
 * the note about keeping the app running. A driver who has done all of it goes straight to the trip.
 */
export function Permissions({ rt, nav, purpose }: { rt: Runtime; nav: Nav; purpose: Purpose }) {
  const [step, setStep] = useState<Step>("check");
  const [notificationsOff, setNotificationsOff] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const begin = async () => {
    setStarting(true);
    setError(null);
    try {
      await startTrip(purpose);
      nav.replace({ name: "recording" });
    } catch (e) {
      setError(e instanceof Error ? e.message : t("error.generic"));
      setStarting(false);
    }
  };

  const afterAccess = (access: LocationAccess) => {
    if (access === "granted") setStep("guide");
    else setStep(access === "ask" ? "denied" : "blocked");
  };

  useEffect(() => {
    let alive = true;
    (async () => {
      const accepted = (await rt.store.getMeta(DISCLOSURE_META)) !== null;
      const access = await locationAccess();
      if (!alive) return;
      if (!accepted) setStep("disclosure");
      else if (access === "granted") void begin();
      else if (access === "blocked") setStep("blocked");
      else setStep("disclosure");
    })().catch(() => alive && setStep("disclosure"));
    return () => {
      alive = false;
    };
    // `begin` is stable enough for a screen that runs this once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rt]);

  const accept = async () => {
    await rt.store.setMeta(DISCLOSURE_META, new Date().toISOString());
    afterAccess(await requestLocation());
    setNotificationsOff(!(await requestNotifications()));
  };

  if (step === "check") {
    return (
      <Screen title={t("disclosure.title")} onBack={nav.back}>
        <Text tone="muted">{t("common.loading")}</Text>
      </Screen>
    );
  }

  if (step === "disclosure") {
    return (
      <Screen
        title={t("disclosure.title")}
        onBack={nav.back}
        footer={
          <>
            <Button
              label={t("disclosure.continue")}
              onPress={() => void accept()}
              testID="disclosure-continue"
            />
            <Button label={t("disclosure.decline")} variant="ghost" onPress={nav.back} />
          </>
        }
      >
        <Text>{t("disclosure.body")}</Text>
      </Screen>
    );
  }

  if (step === "denied" || step === "blocked") {
    const blocked = step === "blocked";
    return (
      <Screen
        title={t(blocked ? "permission.blocked.title" : "permission.denied.title")}
        onBack={nav.back}
        footer={
          blocked ? (
            <Button label={t("permission.settings")} onPress={() => void openSettings()} />
          ) : (
            <Button
              label={t("disclosure.continue")}
              onPress={() => void requestLocation().then(afterAccess)}
            />
          )
        }
      >
        <Banner
          tone="warning"
          title={t(blocked ? "permission.blocked.title" : "permission.denied.title")}
          message={t(blocked ? "permission.blocked.message" : "permission.denied.message")}
        />
      </Screen>
    );
  }

  return (
    <Screen
      title={t("guide.title")}
      onBack={nav.back}
      footer={
        <Button
          label={t("guide.start")}
          onPress={() => void begin()}
          busy={starting}
          testID="guide-start"
        />
      }
    >
      <Text>{t("guide.body")}</Text>
      {notificationsOff ? (
        <Banner
          tone="warning"
          title={t("permission.notifications.title")}
          message={t("permission.notifications.message")}
        />
      ) : null}
      {error ? (
        <Banner tone="danger" title={t("recording.error.start", { message: error })} />
      ) : null}
    </Screen>
  );
}
