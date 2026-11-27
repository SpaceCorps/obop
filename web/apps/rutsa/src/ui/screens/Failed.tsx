import { Button, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { BackHandler } from "react-native";
import { canErase, type DatabaseProblem } from "../../core/open-db.ts";
import { t, type MessageKey } from "../../i18n/index.ts";
import { eraseDatabase } from "../../native/database.ts";
import { stopRecording } from "../../native/recorder.ts";

const DETAIL: Record<Exclude<DatabaseProblem, "other">, MessageKey> = {
  no_cipher: "error.database.detail.no_cipher",
  wrong_key: "error.database.detail.wrong_key",
  newer: "error.database.detail.newer",
};

/**
 * The trips could not be opened (a lost key, a damaged file, a newer version of the app, a build
 * without SQLCipher). Nothing is deleted unless the driver asks for it. The driver can try again;
 * can stop the location updates, which keep running without the app's screens while a trip was
 * recording; and, where it can help (`canErase`), can erase the trips on this phone after two
 * confirmations, which say that the trips are gone for good: the phone holds the only copy. The
 * details say the cause in the driver's words when the app knows it, else the error as it is.
 *
 * After `Failed` of `mileage-app` (`App.tsx`).
 */
export function Failed({
  message,
  problem,
  retry,
}: {
  message: string;
  problem: DatabaseProblem;
  retry: () => void;
}) {
  const [stop, setStop] = useState<"no" | "busy" | "done" | "failed">("no");
  const [details, setDetails] = useState(false);
  const [erase, setErase] = useState<"no" | "first" | "second" | "busy" | "failed">("no");

  const stopNow = () => {
    setStop("busy");
    stopRecording().then(
      () => setStop("done"),
      () => setStop("failed"),
    );
  };

  // On a confirmation the back button of Android cancels the erasing; elsewhere it leaves the app.
  const confirming = erase === "first" || erase === "second";
  useEffect(() => {
    if (!confirming) return;
    const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
      setErase("no");
      return true;
    });
    return () => subscription.remove();
  }, [confirming]);

  const eraseNow = () => {
    setErase("busy");
    eraseDatabase().then(
      () => {
        setErase("no");
        retry();
      },
      () => setErase("failed"),
    );
  };

  if (erase === "first") {
    return (
      <Screen
        title={t("error.database.erase.title")}
        onBack={() => setErase("no")}
        footer={
          <>
            <Button
              label={t("error.database.erase.continue")}
              variant="danger"
              onPress={() => setErase("second")}
              testID="database-erase-continue"
            />
            <Button label={t("common.cancel")} variant="ghost" onPress={() => setErase("no")} />
          </>
        }
      >
        <Text>{t("error.database.erase.first")}</Text>
      </Screen>
    );
  }

  if (erase === "second" || erase === "busy") {
    return (
      <Screen
        title={t("error.database.erase.title")}
        onBack={() => setErase("no")}
        footer={
          <>
            <Button
              label={t("error.database.erase.yes")}
              variant="danger"
              busy={erase === "busy"}
              onPress={eraseNow}
              testID="database-erase-yes"
            />
            <Button
              label={t("common.cancel")}
              variant="ghost"
              disabled={erase === "busy"}
              onPress={() => setErase("no")}
            />
          </>
        }
      >
        <Text tone="danger">{t("error.database.erase.second")}</Text>
      </Screen>
    );
  }

  return (
    <Screen
      title={t("app.title")}
      footer={
        <>
          <Button label={t("common.retry")} onPress={retry} testID="database-retry" />
          <Button
            label={t("error.database.stop")}
            variant="secondary"
            busy={stop === "busy"}
            onPress={stopNow}
            testID="database-stop"
          />
          {canErase(problem) ? (
            <Button
              label={t("error.database.erase")}
              variant="ghost"
              onPress={() => setErase("first")}
              testID="database-erase"
            />
          ) : null}
        </>
      }
    >
      <Text tone="danger" testID="database-error">
        {t("error.database")}
      </Text>
      <Text>{t("error.database.hint")}</Text>
      {stop === "done" ? <Text tone="muted">{t("error.database.stopped")}</Text> : null}
      {stop === "failed" ? <Text tone="danger">{t("error.database.stop.failed")}</Text> : null}
      {erase === "failed" ? <Text tone="danger">{t("error.database.erase.failed")}</Text> : null}
      <Button
        label={t("common.details")}
        variant="ghost"
        onPress={() => setDetails((d) => !d)}
        testID="database-details"
      />
      {details ? (
        <Text tone="muted" testID="database-detail">
          {problem === "other" ? message : t(DETAIL[problem])}
        </Text>
      ) : null}
    </Screen>
  );
}
