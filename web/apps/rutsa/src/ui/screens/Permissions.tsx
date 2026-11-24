import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppState } from "react-native";
import { DISCLOSURE_ACCEPTED, NOTIFICATIONS_ASKED } from "../../core/settings.ts";
import { t } from "../../i18n/index.ts";
import { CannotRecord, startTrip } from "../../native/actions.ts";
import {
  locationAccess,
  notificationsAllowed,
  openLocationSettings,
  openSettings,
  requestLocation,
  requestNotifications,
  type LocationAccess,
} from "../../native/permissions.ts";
import { getSettings } from "../../native/store.ts";
import { onReturnFromAway } from "../foreground.ts";
import { tOs } from "../os-text.ts";
import {
  firstStep,
  notificationsStep,
  stepAfterAccess,
  stepAfterRefusal,
  stepOnReturn,
  waitsForFix,
  type Step,
} from "../permission-step.ts";
import type { Nav, Route } from "../router.ts";

type Next = Extract<Route, { name: "permissions" }>["next"];

/** What the driver was doing when the way led here. */
async function carryOut(next: Next): Promise<void> {
  switch (next) {
    case "start":
      await startTrip();
  }
}

/**
 * The way to a recording (BR-20, BR-36, US-18): the explanation in plain words before the system
 * dialog (what is recorded, when, and that the route stays on this phone); then the permission
 * (precise, not approximate) and the notification of Android 13; then, the first time, the note
 * about keeping the app running. A driver who has done all of it goes straight to the trip.
 * Whatever stops it is shown with a way on (Open settings) and Close, which goes back to where the
 * driver was; coming back from the settings checks again and goes on by itself (US-18 AC5).
 *
 * After `Permissions` of `mileage-app` (`src/ui/screens/Permissions.tsx`).
 */
export function Permissions({ nav, next }: { nav: Nav; next: Next }) {
  const [step, setStep] = useState<Step>("check");
  const [notificationsOff, setNotificationsOff] = useState(false);
  const [starting, setStarting] = useState(false);
  const [failed, setFailed] = useState(false);
  // A system dialog of the app is open: on Android it sends the app to the background, and that is
  // not a trip to the settings (`onReturnFromAway`).
  const asking = useRef(false);
  const current = useRef<Step>("check");
  useEffect(() => {
    current.current = step;
  }, [step]);

  /** Asks the system, with the return of its dialog not taken for a return from the settings. */
  const ask = useCallback(async <T,>(question: () => Promise<T>): Promise<T> => {
    asking.current = true;
    try {
      return await question();
    } finally {
      asking.current = false;
    }
  }, []);

  /**
   * Whether the notification of a recording can be shown. The dialog of Android 13 comes once in
   * the life of the app, after a tap (`notificationsStep`); otherwise the answer is only read.
   */
  const notifications = useCallback(
    async (byTap: boolean): Promise<boolean> => {
      const settings = await getSettings();
      const asked = (await settings.get(NOTIFICATIONS_ASKED)) !== null;
      if (notificationsStep(asked, byTap) === "read") return notificationsAllowed();
      await settings.set(NOTIFICATIONS_ASKED, new Date().toISOString());
      return ask(requestNotifications);
    },
    [ask],
  );

  /** Starts the trip; `byTap` is false for the start that goes on by itself after the settings. */
  const begin = useCallback(
    async (byTap: boolean) => {
      setStarting(true);
      setFailed(false);
      try {
        setNotificationsOff(!(await notifications(byTap).catch(() => true)));
        await carryOut(next);
        // The recording route (the temporary panel until D4) replaces this screen; Back is home.
        nav.replace({ name: "recording" });
      } catch (e) {
        setStarting(false);
        if (e instanceof CannotRecord) {
          const access = await locationAccess().catch((): LocationAccess => "blocked");
          setStep(stepAfterRefusal(e.reason, access));
          return;
        }
        // The reason is shown on the screen with the Start button, not on a loading screen.
        setFailed(true);
        setStep("guide");
      }
    },
    [notifications, nav, next],
  );

  // The first step: the explanation until it was read, else straight on or what stands in the way.
  useEffect(() => {
    let alive = true;
    (async () => {
      const accepted = (await (await getSettings()).get(DISCLOSURE_ACCEPTED)) !== null;
      const access = await locationAccess();
      if (!alive) return;
      const first = firstStep(accepted, access);
      // The driver pressed Start to come here.
      if (first === "begin") void begin(true);
      else setStep(first);
    })().catch(() => alive && setStep("disclosure"));
    return () => {
      alive = false;
    };
    // Once, when the screen opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Back from the settings: check again, and go on by itself when location is allowed now.
  useEffect(
    () =>
      onReturnFromAway(
        AppState,
        () => asking.current,
        () => {
          if (!waitsForFix(current.current)) return;
          locationAccess().then(
            (access) => {
              const after = stepOnReturn(access);
              // No dialog here: the driver went to the settings for the location.
              if (after === "begin") void begin(false);
              else setStep(after);
            },
            () => undefined,
          );
        },
      ),
    [begin],
  );

  const accept = async () => {
    await (await getSettings()).set(DISCLOSURE_ACCEPTED, new Date().toISOString());
    const access = await ask(requestLocation);
    setStep(stepAfterAccess(access));
    if (access === "granted") setNotificationsOff(!(await notifications(true)));
  };

  const askAgain = async () => {
    const access = await ask(requestLocation);
    setStep(stepAfterAccess(access));
    if (access === "granted") setNotificationsOff(!(await notifications(true)));
  };

  const close = <Button label={t("common.close")} variant="ghost" onPress={nav.back} />;

  if (step === "check") {
    return (
      <Screen title={t("disclosure.title")} onBack={nav.back} scroll={false}>
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
              onPress={() => void accept().catch(() => setStep("disclosure"))}
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

  if (step === "denied") {
    return (
      <Screen
        title={t("permission.denied.title")}
        onBack={nav.back}
        footer={
          <>
            <Button
              label={t("disclosure.continue")}
              onPress={() => void askAgain()}
              testID="denied-continue"
            />
            {close}
          </>
        }
      >
        <Banner
          tone="warning"
          title={t("permission.denied.title")}
          message={t("permission.denied.message")}
        />
      </Screen>
    );
  }

  if (step === "blocked" || step === "approximate" || step === "services") {
    const services = step === "services";
    const title =
      step === "blocked"
        ? t("permission.blocked.title")
        : step === "approximate"
          ? t("permission.approximate.title")
          : t("permission.services.title");
    const message =
      step === "blocked"
        ? t("permission.blocked.message")
        : step === "approximate"
          ? t("permission.approximate.message")
          : tOs("permission.services.message");
    return (
      <Screen
        title={title}
        onBack={nav.back}
        footer={
          <>
            <Button
              label={t("permission.settings")}
              busy={starting}
              onPress={() => void (services ? openLocationSettings() : openSettings())}
              testID={`${step}-settings`}
            />
            {close}
          </>
        }
      >
        <Banner tone="warning" title={title} message={message} testID={`${step}-message`} />
      </Screen>
    );
  }

  return (
    <Screen
      title={t("guide.title")}
      onBack={nav.back}
      footer={
        <>
          <Button
            label={t("guide.start")}
            onPress={() => void begin(true)}
            busy={starting}
            testID="guide-start"
          />
          {close}
        </>
      }
    >
      <Text>{tOs("guide.body")}</Text>
      {notificationsOff ? (
        <Banner
          tone="warning"
          title={t("permission.notifications.title")}
          message={t("permission.notifications.message")}
          action={{ label: t("permission.settings"), onPress: () => void openSettings() }}
          testID="notifications-off"
        />
      ) : null}
      {failed ? (
        <Banner tone="danger" title={t("recording.error.start")} testID="start-failed" />
      ) : null}
    </Screen>
  );
}
