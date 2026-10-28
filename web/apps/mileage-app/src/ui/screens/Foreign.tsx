import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { useEffect, useState } from "react";
import { Alert } from "react-native";
import type { Binding } from "../../core/identity.ts";
import { unsentSentence } from "../../core/present.ts";
import { t } from "../../i18n/index.ts";
import { signOut, stopTrip } from "../../native/actions.ts";
import type { Runtime } from "../../native/runtime.ts";
import { useOutbox } from "../hooks.ts";

/**
 * Shown instead of every other screen when the phone holds trips that were recorded under another
 * sign-in (another person, or the same person on another server). Nothing of theirs is uploaded or
 * shown, and nothing is deleted until the driver chooses: sign out and leave them for their owner,
 * or delete them and go on. A trip of theirs that is still recording is stopped first (on the
 * phone), because the location service would otherwise go on writing into it.
 */
export function Foreign({
  rt,
  binding,
}: {
  rt: Runtime;
  binding: Extract<Binding, { kind: "foreign" }>;
}) {
  const { active } = useOutbox(rt);
  const [stopped, setStopped] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  const openId = active?.clientTripId ?? null;
  useEffect(() => {
    if (openId === null) return;
    stopTrip(openId).then(
      () => setStopped(true),
      () => setFailed(true),
    );
  }, [openId]);

  const run = (work: () => Promise<void>) => {
    setBusy(true);
    setFailed(false);
    work().then(
      () => setBusy(false),
      () => {
        setBusy(false);
        setFailed(true);
      },
    );
  };

  const confirmDelete = () => {
    Alert.alert(
      t("foreign.delete.title"),
      unsentSentence("foreign.delete.message", binding.unsent),
      [
        { text: t("common.cancel"), style: "cancel" },
        {
          text: t("foreign.delete.confirm"),
          style: "destructive",
          onPress: () => run(() => rt.takeOverOutbox()),
        },
      ],
    );
  };

  return (
    <Screen
      title={t("foreign.title")}
      footer={
        <>
          <Button
            label={t("foreign.keep")}
            hint={t("foreign.keep.hint")}
            onPress={() => run(() => signOut(true))}
            busy={busy}
            testID="foreign-keep"
          />
          <Button
            label={t("foreign.delete")}
            variant="danger"
            onPress={confirmDelete}
            disabled={busy}
            testID="foreign-delete"
          />
        </>
      }
    >
      <Banner
        tone="warning"
        title={t("foreign.title")}
        message={unsentSentence("foreign.message", binding.unsent)}
        testID="foreign-banner"
      />
      {stopped ? <Text tone="muted">{t("foreign.stopped")}</Text> : null}
      {failed ? <Banner tone="danger" title={t("error.generic")} /> : null}
    </Screen>
  );
}
