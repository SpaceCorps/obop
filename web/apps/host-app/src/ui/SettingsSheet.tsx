import { Banner, Button, Row, Screen, Text, useTheme } from "@obop/ui-native";
import type { ReactNode } from "react";
import { StyleSheet, View } from "react-native";
import { addressText, isCleartext } from "../core/address.ts";
import { isTestBuild, type Build } from "../core/build.ts";
import { dayText, type ServerRecord } from "../core/server-record.ts";
import { t } from "../i18n/index.ts";
import { ButtonRow } from "./ButtonRow.tsx";

export type SettingsSheetProps = {
  server: ServerRecord;
  build: Build;
  /** The version of the app and its version code: `0.1.0 (1)`. */
  appVersion: string;
  onClose: () => void;
  onOpenInBrowser: () => void;
  onChangeServer: () => void;
};

function Group({ title, children }: { title: string; children: ReactNode }) {
  const theme = useTheme();
  return (
    <View style={{ gap: theme.space[2] }}>
      <Text variant="heading">{title}</Text>
      <View
        style={{
          borderRadius: theme.radius.lg,
          borderWidth: 1,
          borderColor: theme.c.border,
          overflow: "hidden",
        }}
      >
        {children}
      </View>
    </View>
  );
}

/**
 * "This app": which server the app shows, what kind of build it is, and the two things only the
 * app can do, Open in browser and Change server. It opens from the host bar, from the page's
 * account menu (`host.settings`) and from the error screen, and lies over the WebView, which
 * stays mounted under it. The two buttons are pinned at the bottom, under the thumb; Android's
 * Back closes the screen.
 */
export function SettingsSheet({
  server,
  build,
  appVersion,
  onClose,
  onOpenInBrowser,
  onChangeServer,
}: SettingsSheetProps) {
  const test = isTestBuild(build);
  return (
    <View style={StyleSheet.absoluteFill} testID="settings-sheet">
      <Screen
        title={t("settings.title")}
        onBack={onClose}
        backAtStart
        compactInLandscape
        footerToEdges
        footer={
          <ButtonRow>
            <Button
              label={t("settings.openInBrowser")}
              variant="secondary"
              onPress={onOpenInBrowser}
              testID="settings-open-in-browser"
            />
            <Button
              label={t("settings.changeServer")}
              variant="danger"
              onPress={onChangeServer}
              testID="settings-change-server"
            />
          </ButtonRow>
        }
      >
        <Group title={t("settings.server")}>
          <Row
            title={t("settings.address")}
            subtitle={addressText(server)}
            testID="settings-address"
          />
          <Row
            title={t("settings.connection")}
            value={
              isCleartext(server.origin)
                ? t("settings.connection.http")
                : t("settings.connection.https")
            }
            testID="settings-connection"
          />
          <Row title={t("settings.serverVersion")} value={server.coreVersion} />
          <Row title={t("settings.added")} value={dayText(server.addedAt)} />
        </Group>
        <Group title={t("settings.app")}>
          <Row title={t("settings.appVersion")} value={appVersion} testID="settings-version" />
          <Row
            title={t("settings.build")}
            value={test ? t("settings.build.test") : t("settings.build.plain")}
            testID="settings-build"
          />
          <Row
            title={t("settings.cleartext")}
            value={build.cleartext ? t("settings.cleartext.on") : t("settings.cleartext.off")}
            testID="settings-cleartext"
          />
          <Row
            title={t("settings.inspectable")}
            value={
              build.webviewDebug ? t("settings.inspectable.on") : t("settings.inspectable.off")
            }
            testID="settings-inspectable"
          />
        </Group>
        {test ? (
          <Banner
            tone="warning"
            title={t("settings.build.test")}
            message={t("settings.test.message")}
            testID="settings-test-build"
          />
        ) : null}
      </Screen>
    </View>
  );
}
