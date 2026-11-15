import { Banner, Button, Field, Row, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { Alert } from "react-native";
import { unsentSentence } from "../../core/present.ts";
import { hasWork } from "../../core/store.ts";
import { t, tp } from "../../i18n/index.ts";
import { signOut, unsentSummary } from "../../native/actions.ts";
import { databaseInfo } from "../../native/store.ts";
import {
  readDevice,
  removeThisDevice,
  renameDevice,
  type DeviceView,
} from "../../native/devices.ts";
import { appVersion } from "../../native/platform-deps.ts";
import type { Runtime } from "../../native/runtime.ts";
import { announce } from "../announce.ts";
import { useOutbox, useSyncStatus } from "../hooks.ts";
import type { Nav } from "../router.ts";

export function Account({ rt, nav }: { rt: Runtime; nav: Nav }) {
  const { trips } = useOutbox(rt);
  const sync = useSyncStatus(rt);
  const [device, setDevice] = useState<DeviceView | null>(null);
  const [deviceError, setDeviceError] = useState(false);
  const [leaveError, setLeaveError] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [name, setName] = useState("");
  const [saved, setSaved] = useState(false);
  const [cipher, setCipher] = useState<string | null>(null);

  useEffect(() => {
    readDevice(rt).then(
      (d) => {
        setDevice(d);
        setName(d.name);
      },
      () => setDeviceError(true),
    );
    databaseInfo().then(
      (info) => setCipher(info.cipherVersion),
      () => undefined,
    );
  }, [rt]);

  // "Everything is uploaded" is only said when no trip of this phone is waiting or stuck.
  const mine = trips.filter((x) => x.origin === "device");
  const pending = mine.filter((x) => hasWork(x)).length;
  const stuck = mine.filter((x) => x.problem !== null && x.problem.code !== "incomplete").length;
  const report = sync.report;
  const count = (): string =>
    stuck > 0
      ? tp("account.upload.stuck", stuck)
      : pending > 0
        ? tp("account.upload.pending", pending)
        : t("account.upload.idle");
  const upload =
    stuck > 0 || report === null || report.state === "idle"
      ? count()
      : report.state === "offline"
        ? t("account.upload.offline")
        : report.state === "rate_limited"
          ? t("account.upload.limited")
          : report.state === "unavailable"
            ? t("account.upload.unavailable")
            : report.state === "signed_out"
              ? t("account.upload.signed_out")
              : count();

  const save = () => {
    setSaved(false);
    renameDevice(rt, name).then(
      () => {
        setSaved(true);
        announce(t("announce.saved"));
      },
      () => setDeviceError(true),
    );
  };

  /** Signs out (or removes this phone), keeping or deleting the trips that exist only on the phone. */
  const go = (remove: boolean, keepUnsent: boolean) => {
    setLeaving(true);
    setLeaveError(false);
    const done = remove ? removeThisDevice(rt, keepUnsent) : signOut(keepUnsent);
    done.then(
      () => nav.home(),
      () => {
        setLeaving(false);
        setLeaveError(true);
      },
    );
  };

  const leave = async (remove: boolean) => {
    const unsent = await unsentSummary();
    if (unsent.trips === 0) {
      Alert.alert(
        t(remove ? "account.device.revoke.title" : "account.signout.title"),
        t(remove ? "account.device.revoke.message" : "account.signout.message"),
        [
          { text: t("common.cancel"), style: "cancel" },
          {
            text: t(remove ? "account.device.revoke" : "account.signout"),
            style: "destructive",
            onPress: () => go(remove, false),
          },
        ],
      );
      return;
    }
    // Trips that exist only on this phone: say how many, and let the driver keep them (the default
    // and the safe choice) or delete them after a second, explicit confirmation.
    Alert.alert(
      t("account.leave.pending.title"),
      unsentSentence("account.leave.pending.message", unsent),
      [
        { text: t("common.cancel"), style: "cancel" },
        {
          text: t("account.leave.delete"),
          style: "destructive",
          onPress: () =>
            Alert.alert(
              t("account.leave.delete.title"),
              unsentSentence("account.leave.delete.message", unsent),
              [
                { text: t("common.cancel"), style: "cancel" },
                {
                  text: t(remove ? "account.leave.delete.remove" : "account.leave.delete.signout"),
                  style: "destructive",
                  onPress: () => go(remove, false),
                },
              ],
            ),
        },
        {
          text: t(remove ? "account.leave.keep.remove" : "account.leave.keep.signout"),
          onPress: () => go(remove, true),
        },
      ],
    );
  };

  return (
    <Screen title={t("account.title")} onBack={nav.back}>
      <Row title={t("account.server")} value={rt.session.config()?.baseUrl ?? ""} testID="server" />
      <Row title={t("account.version")} value={appVersion()} />
      <Row
        title={t("account.storage")}
        value={cipher === null ? "" : t("account.storage.value", { version: cipher })}
        testID="storage"
      />
      <Row title={t("account.upload")} value={upload} testID="upload" />
      <Button
        label={t("account.upload.now")}
        variant="secondary"
        onPress={() => rt.sync.kick()}
        busy={sync.running}
      />

      <Text variant="heading">{t("account.device")}</Text>
      {deviceError ? <Banner tone="warning" title={t("account.device.error")} /> : null}
      {device ? (
        <>
          <Field
            label={t("account.device.name")}
            value={name}
            onChangeText={(text) => {
              setName(text);
              setSaved(false);
            }}
            maxLength={80}
            testID="device-name"
          />
          <Button
            label={saved ? t("account.device.saved") : t("account.device.save")}
            variant="secondary"
            disabled={name.trim() === "" || name.trim() === device.name}
            onPress={save}
          />
          <Row title={device.platform} subtitle={device.appVersion} value={device.state} />
        </>
      ) : null}
      <Text tone="muted">{t("account.device.list")}</Text>

      {leaveError ? (
        <Banner tone="danger" title={t("account.leave.error")} testID="leave-error" />
      ) : null}
      <Button
        label={t("account.signout")}
        onPress={() => void leave(false)}
        busy={leaving}
        testID="signout"
      />
      <Button
        label={t("account.device.revoke")}
        variant="danger"
        onPress={() => void leave(true)}
        disabled={leaving}
        testID="remove-device"
      />
    </Screen>
  );
}
