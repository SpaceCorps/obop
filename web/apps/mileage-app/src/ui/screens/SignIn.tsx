import { Banner, Button, Field, Screen, Text } from "@obop/ui-native";
import Constants from "expo-constants";
import { useEffect, useState } from "react";
import { t, type MessageKey } from "../../i18n/index.ts";
import { SignInError, signIn, type SignInFailure } from "../../native/auth.ts";
import type { Runtime } from "../../native/runtime.ts";

/** A build made for a test instance may carry the address to start from (`extra.defaultServer`). */
const defaultServer = (): string => {
  const value: unknown = Constants.expoConfig?.extra?.defaultServer;
  return typeof value === "string" ? value : "";
};

/**
 * `onBack` is given while a trip is still being recorded: the driver may go back to the recording
 * (and its Stop button) instead of signing in.
 */
export function SignIn({
  rt,
  expired,
  onBack,
}: {
  rt: Runtime;
  expired: boolean;
  onBack?: () => void;
}) {
  const [address, setAddress] = useState(rt.session.config()?.baseUrl ?? defaultServer());
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<SignInFailure | null>(null);
  // Whether the last sign-out could not tell the instance; said while this screen is open.
  const [untold] = useState(() => rt.signOutUntold());
  useEffect(() => () => rt.setSignOutUntold(false), [rt]);

  const submit = async () => {
    setBusy(true);
    setFailure(null);
    try {
      await signIn(rt, address);
    } catch (error) {
      setFailure(error instanceof SignInError ? error.reason : "failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen
      title={t("signin.title")}
      footer={
        <>
          <Button
            label={busy ? t("signin.busy") : t("signin.button")}
            onPress={() => void submit()}
            busy={busy}
            disabled={address.trim() === ""}
            testID="signin-button"
          />
          {onBack ? (
            <Button
              label={t("signin.back")}
              variant="ghost"
              onPress={onBack}
              testID="signin-back"
            />
          ) : null}
        </>
      }
    >
      {expired ? (
        <Banner
          tone="warning"
          title={t("signin.expired.title")}
          message={t("signin.expired.message")}
        />
      ) : null}
      {untold ? (
        <Banner
          tone="warning"
          title={t("signin.untold.title")}
          message={t("signin.untold.message")}
          testID="signout-untold"
        />
      ) : null}
      <Text>{t("signin.intro")}</Text>
      <Field
        label={t("signin.address")}
        value={address}
        onChangeText={setAddress}
        helper={t("signin.address.helper")}
        keyboardType="url"
        autoCapitalize="none"
        autoCorrect={false}
        testID="signin-address"
      />
      {failure ? (
        <Banner
          tone={failure === "cancelled" ? "info" : "danger"}
          title={t(`signin.error.${failure}` as MessageKey)}
        />
      ) : null}
    </Screen>
  );
}
