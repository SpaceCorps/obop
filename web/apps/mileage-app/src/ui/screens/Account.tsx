import { Banner, Button, Field, Row, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { Alert } from "react-native";
import { hasWork } from "../../core/store.ts";
import { t } from "../../i18n/index.ts";
import { signOut, unsentTrips } from "../../native/actions.ts";
import { databaseInfo } from "../../native/store.ts";
import {
  readDevice,
  removeThisDevice,
  renameDevice,
  type DeviceView,
} from "../../native/devices.ts";
import { appVersion } from "../../native/platform-deps.ts";
import type { Runtime } from "../../native/runtime.ts";
import { useOutbox, useSyncStatus } from "../hooks.ts";
import type { Nav } from "../router.ts";

export function Account({ rt, nav }: { rt: Runtime; nav: Nav }) {
  const { trips } = useOutbox(rt);
  const sync = useSyncStatus(rt);
  const [device, setDevice] = useState<DeviceView | null>(null);
  const [deviceError, setDeviceError] = useState(false);
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

  const pending = trips.filter((x) => x.origin === "device" && hasWork(x)).length;
  const report = sync.report;
  const upload =
    report === null || report.state === "idle"
      ? pending > 0
        ? t("account.upload.pending", { count: pending })
        : t("account.upload.idle")
      : report.state === "offline"
        ? t("account.upload.offline")
        : report.state === "rate_limited"
          ? t("account.upload.limited")
          : report.state === "unavailable"
            ? t("account.upload.unavailable")
            : report.state === "signed_out"
              ? t("account.upload.signed_out")
              : pending > 0
                ? t("account.upload.pending", { count: pending })
                : t("account.upload.idle");

  const save = () => {
    setSaved(false);
    renameDevice(rt, name).then(
      () => setSaved(true),
      () => setDeviceError(true),
    );
  };

  const leave = async (remove: boolean) => {
    const unsent = await unsentTrips();
    const go = () => {
      const done = remove ? removeThisDevice(rt) : signOut();
      done.then(
        () => nav.home(),
        () => setDeviceError(true),
      );
    };
    if (unsent.length > 0) {
      Alert.alert(
        t("account.signout.pending.title"),
        t("account.signout.pending.message", { count: unsent.length }),
        [
          { text: t("common.cancel"), style: "cancel" },
          { text: t("account.signout.delete"), style: "destructive", onPress: go },
        ],
      );
      return;
    }
    Alert.alert(
      t(remove ? "account.device.revoke.title" : "account.signout.title"),
      t(remove ? "account.device.revoke.message" : "account.signout.message"),
      [
        { text: t("common.cancel"), style: "cancel" },
        {
          text: t(remove ? "account.device.revoke" : "account.signout"),
          style: "destructive",
          onPress: go,
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

      <Button label={t("account.signout")} onPress={() => void leave(false)} testID="signout" />
      <Button
        label={t("account.device.revoke")}
        variant="danger"
        onPress={() => void leave(true)}
        testID="remove-device"
      />
    </Screen>
  );
}
