import { Banner, Button, Field, Screen, Text } from "@obop/ui-native";
import { isTestBuild, type Build } from "../core/build.ts";
import type { AddressNote } from "../core/shell.ts";
import { addressSentence } from "../core/words.ts";
import { t } from "../i18n/index.ts";

export type AddressScreenProps = {
  build: Build;
  /** The text of the field. */
  value: string;
  note: AddressNote | null;
  /** The address is being checked. */
  busy: boolean;
  onChange: (text: string) => void;
  onConnect: () => void;
  /** The button under "This address forwards to". */
  onUseForwarded: () => void;
};

/**
 * The first screen: one field for the address of the instance and Connect. The field is at the top
 * so that the keyboard does not cover it, and Connect is pinned at the bottom, under the thumb.
 * The screen never opens the page on a refused address; what it says is why.
 */
export function AddressScreen({
  build,
  value,
  note,
  busy,
  onChange,
  onConnect,
  onUseForwarded,
}: AddressScreenProps) {
  const sentence = addressSentence(note, build);
  return (
    <Screen
      title={t("address.title")}
      footer={
        <Button
          label={busy ? t("address.connecting") : t("address.connect")}
          onPress={onConnect}
          busy={busy}
          disabled={value.trim() === ""}
          testID="address-connect"
        />
      }
    >
      <Text tone="muted">{t("address.lead")}</Text>
      <Field
        label={t("address.label")}
        value={value}
        onChangeText={onChange}
        placeholder={t("address.placeholder")}
        helper={t("address.helper")}
        {...(sentence === null ? {} : { error: t(sentence.key, sentence.params) })}
        editable={!busy}
        keyboardType="url"
        autoCapitalize="none"
        autoCorrect={false}
        maxLength={300}
        testID="address-field"
      />
      {note?.kind === "forwarded" ? (
        <Banner
          tone="info"
          title={t("address.forwarded.title", { origin: note.origin })}
          message={t("address.forwarded.message")}
          action={{
            label: t("address.forwarded.use", { origin: note.origin }),
            onPress: onUseForwarded,
          }}
          testID="address-forwarded"
        />
      ) : null}
      {isTestBuild(build) ? (
        <Banner
          tone="warning"
          title={t("address.test.title")}
          message={t("address.test.message")}
          testID="address-test-build"
        />
      ) : null}
    </Screen>
  );
}
