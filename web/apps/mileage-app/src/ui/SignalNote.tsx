import { Banner, Text } from "@obop/ui-native";
import { signalBannerProps } from "../core/present.ts";
import type { Progress } from "../core/progress.ts";

/**
 * The note on the Recording screen about a distance that stays at 0 m: a warning banner whose words
 * are fixed per kind, and under it a muted line with the counts. The counts are not in the banner,
 * because a screen reader reads a banner again whenever its text changes. Nothing is drawn before
 * the stored positions have been read once (`measured` is `null`) or when there is nothing to say.
 * It has no state and no hook, so a test can call it as a function and read the elements it makes.
 */
export function SignalNote({ measured }: { measured: Pick<Progress, "signal"> | null }) {
  const props = signalBannerProps(measured);
  if (props === null) return null;
  return (
    <>
      <Banner tone={props.tone} title={props.title} message={props.message} testID={props.testID} />
      <Text tone="muted" testID={`${props.testID}-detail`}>
        {props.detail}
      </Text>
    </>
  );
}
