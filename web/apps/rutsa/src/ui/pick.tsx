import DateTimePicker, { DateTimePickerAndroid } from "@react-native-community/datetimepicker";
import { Platform } from "react-native";

/**
 * The date and the time of a trip added by hand (D7), chosen with the pickers of the system
 * (`@react-native-community/datetimepicker`): on Android a dialog that a tap on the row opens
 * (`pickOnAndroid`), on iOS the compact picker that stands in the row itself (`IosPicker`). Times
 * are 24 h. What is chosen is a moment in the phone's time zone; the screen keeps it as text.
 */

export type PickMode = "date" | "time";

/** The pickers stand in the row on iOS; on Android the row opens a dialog. */
export const pickerInRow = Platform.OS === "ios";

/** Opens the dialog of Android; the moment chosen, or `null` when it is dismissed. */
export function pickOnAndroid(
  mode: PickMode,
  value: Date,
  maximumDate?: Date,
): Promise<Date | null> {
  return new Promise((resolve) => {
    DateTimePickerAndroid.open({
      value,
      mode,
      is24Hour: true,
      ...(maximumDate ? { maximumDate } : {}),
      onChange: (event, date) => resolve(event.type === "set" && date ? date : null),
    });
  });
}

/** The compact picker of iOS, in the row. */
export function IosPicker({
  mode,
  value,
  maximumDate,
  onPick,
  testID,
}: {
  mode: PickMode;
  value: Date;
  maximumDate?: Date;
  onPick: (date: Date) => void;
  testID?: string;
}) {
  return (
    <DateTimePicker
      value={value}
      mode={mode}
      display="compact"
      // `is24Hour` is a property of Android only; on iOS a 24 h locale gives the clock of 24 h.
      locale="en-GB"
      {...(maximumDate ? { maximumDate } : {})}
      onChange={(event, date) => {
        if (event.type === "set" && date) onPick(date);
      }}
      testID={testID}
    />
  );
}
