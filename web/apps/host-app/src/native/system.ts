import { Alert, Linking } from "react-native";
import { t } from "../i18n/index.ts";

/**
 * Hands an address to the system: the browser for http and https, the mail app, the dialler. A
 * phone without an app for it does nothing; the page is told nothing either way.
 */
export function openExternal(url: string): void {
  Linking.openURL(url).catch(() => undefined);
}

/**
 * The one confirmation before Change server, as a dialog of the system. Whoever holds the unlocked
 * phone can confirm it: a device prompt is not part of this slice.
 */
export function confirmChangeServer(host: string): Promise<boolean> {
  return new Promise((resolve) => {
    Alert.alert(
      t("change.title"),
      t("change.message", { host }),
      [
        { text: t("change.cancel"), style: "cancel", onPress: () => resolve(false) },
        { text: t("change.confirm"), style: "destructive", onPress: () => resolve(true) },
      ],
      { cancelable: true, onDismiss: () => resolve(false) },
    );
  });
}
