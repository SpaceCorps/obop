import { Banner, Button, Screen, Text } from "@obop/ui-native";
import { StyleSheet, View } from "react-native";
import type { WebProblem } from "../core/shell.ts";
import { errorSentence } from "../core/words.ts";
import { t } from "../i18n/index.ts";
import { ButtonRow } from "./ButtonRow.tsx";

export type ErrorScreenProps = {
  /** The stored origin of the server. */
  origin: string;
  problem: WebProblem;
  /** Retry was tapped and its load is running. */
  retrying: boolean;
  onRetry: () => void;
  onChangeServer: () => void;
  onThisApp: () => void;
};

/**
 * The native screen for a server that cannot be reached or does not answer as it should. It lies
 * over the WebView, which stays mounted under it, or the session of the page would end. The
 * library's own error and loading views are never shown.
 */
export function ErrorScreen({
  origin,
  problem,
  retrying,
  onRetry,
  onChangeServer,
  onThisApp,
}: ErrorScreenProps) {
  const { main, hint, detail } = errorSentence(problem, origin);
  return (
    <View style={StyleSheet.absoluteFill} testID="error-screen">
      <Screen
        title={t("error.title")}
        compactInLandscape
        footerToEdges
        footer={
          <ButtonRow>
            <Button
              label={t("error.retry")}
              onPress={onRetry}
              busy={retrying}
              testID="error-retry"
            />
            <Button
              label={t("error.changeServer")}
              variant="secondary"
              onPress={onChangeServer}
              disabled={retrying}
              testID="error-change-server"
            />
          </ButtonRow>
        }
      >
        <Banner
          tone="danger"
          title={t(main.key, main.params)}
          {...(hint === null ? {} : { message: t(hint.key, hint.params) })}
          testID="error-reason"
        />
        {detail === null ? null : (
          <Text variant="caption" tone="muted" testID="error-detail">
            {t("error.detail", { detail })}
          </Text>
        )}
        <Button
          label={t("error.thisApp")}
          variant="ghost"
          onPress={onThisApp}
          testID="error-this-app"
        />
      </Screen>
    </View>
  );
}
