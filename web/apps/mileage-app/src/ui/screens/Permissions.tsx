import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import type { Purpose } from "../../core/types.ts";
import { t } from "../../i18n/index.ts";
import { CannotRecord, startTrip } from "../../native/actions.ts";
import {
  locationAccess,
  openSettings,
  requestLocation,
  requestNotifications,
  type LocationAccess,
} from "../../native/permissions.ts";
import type { Runtime } from "../../native/runtime.ts";
import { tOs } from "../os-text.ts";
import type { Nav } from "../router.ts";

/** The key under which the outbox remembers that the driver has read the disclosure. */
export const DISCLOSURE_META = "location_disclosure_accepted";

type Step = "check" | "disclosure" | "denied" | "blocked" | "approximate" | "services" | "guide";

/**
 * Before the first Start: the disclosure in the app, in plain words, before the system dialog
 * (Google Play asks for it when location is collected in the background; it is shown on iOS too, with its own wording
 * where the Android one names a notification); then the permission
 * (precise, not approximate); then the note about keeping the app running. A driver who has done
 * all of it goes straight to the trip. Whatever stops a Start is shown here with a way on: the
 * location switch of the phone, an approximate-only permission, a refused permission, a failure.
 */
export function Permissions({ rt, nav, purpose }: { rt: Runtime; nav: Nav; purpose: Purpose }) {
  const [step, setStep] = useState<Step>("check");
  const [notificationsOff, setNotificationsOff] = useState(false);
  const [starting, setStarting] = useState(false);
  const [failed, setFailed] = useState(false);

  const afterAccess = (access: LocationAccess) => {
    if (access === "granted") setStep("guide");
    else if (access === "approximate") setStep("approximate");
    else setStep(access === "ask" ? "denied" : "blocked");
  };

  const begin = async () => {
    setStarting(true);
    setFailed(false);
    try {
      await startTrip(purpose);
      nav.replace({ name: "recording" });
    } catch (e) {
      setStarting(false);
      if (e instanceof CannotRecord) {
        if (e.reason === "services_off") setStep("services");
        else if (e.reason === "approximate") setStep("approximate");
        else afterAccess(await locationAccess().catch((): LocationAccess => "blocked"));
        return;
      }
      // The reason is shown on the screen with the Start button, not on a loading screen.
      setFailed(true);
      setStep("guide");
    }
  };

  useEffect(() => {
    let alive = true;
    (async () => {
      const accepted = (await rt.store.getMeta(DISCLOSURE_META)) !== null;
      const access = await locationAccess();
      if (!alive) return;
      if (!accepted) setStep("disclosure");
      else if (access === "granted") void begin();
      else if (access === "approximate") setStep("approximate");
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
        <Text>{tOs("disclosure.body")}</Text>
      </Screen>
    );
  }

  if (step === "services") {
    return (
      <Screen
        title={t("permission.services.title")}
        onBack={nav.back}
        footer={
          <Button
            label={t("common.retry")}
            onPress={() => void begin()}
            busy={starting}
            testID="services-retry"
          />
        }
      >
        <Banner
          tone="warning"
          title={t("permission.services.title")}
          message={tOs("permission.services.message")}
        />
      </Screen>
    );
  }

  if (step === "approximate") {
    return (
      <Screen
        title={t("permission.approximate.title")}
        onBack={nav.back}
        footer={
          <>
            <Button label={t("permission.settings")} onPress={() => void openSettings()} />
            <Button
              label={t("common.retry")}
              variant="secondary"
              onPress={() => void locationAccess().then(afterAccess)}
              testID="approximate-retry"
            />
          </>
        }
      >
        <Banner
          tone="warning"
          title={t("permission.approximate.title")}
          message={t("permission.approximate.message")}
        />
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
      <Text>{tOs("guide.body")}</Text>
      {notificationsOff ? (
        <Banner
          tone="warning"
          title={t("permission.notifications.title")}
          message={t("permission.notifications.message")}
        />
      ) : null}
      {failed ? (
        <Banner tone="danger" title={t("recording.error.start")} testID="start-failed" />
      ) : null}
    </Screen>
  );
}
