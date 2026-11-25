import { DETAIL_MAX } from "../core/details.ts";
import type { Trip } from "../core/types.ts";
import { t } from "../i18n/index.ts";
import { updateTripDetails } from "../native/actions.ts";
import { InlineText } from "./InlineText.tsx";
import { tripName, typedName } from "./trip-label.ts";

/** The default name of an untitled trip: "Untitled trip 12" ("Trip" for one with no number). */
export function untitledName(trip: Pick<Trip, "name" | "number">): string {
  return tripName({ name: null, number: trip.number }, (number) =>
    number === null ? t("trip.title") : t("trip.untitled", { number }),
  );
}

/**
 * The name of a trip in the current widget, typed in place (D6). An untitled trip shows its
 * default ("Untitled trip 12") as a placeholder: a tap gives an empty field, and a field left
 * empty keeps the trip untitled, with its number. On the home screen it stores the name itself
 * when the field loses the focus, with the other details as they are; on the trip's screen it
 * hands what is typed to the screen (`onChange`), which keeps it at Done.
 */
export function TripNameInput({
  trip,
  onChange,
  editable = true,
  placeholderTone = "text",
}: {
  trip: Pick<Trip, "id" | "name" | "number" | "from" | "to" | "note">;
  /** What is typed, as it is typed (the trip's screen keeps it until Done); without it the name is stored when the field loses the focus. */
  onChange?: (text: string) => void;
  /** `false`: read only (the trip's screen until Edit is pressed). */
  editable?: boolean;
  /** The default name in black (`text`, the trip's name) or grey (`muted`, an empty field). */
  placeholderTone?: "muted" | "text";
}) {
  const placeholder = untitledName(trip);
  const store = (text: string) => {
    // The default typed as it is, or nothing, is no name: the trip keeps its number.
    const name = typedName(text, placeholder);
    updateTripDetails(trip.id, { name, from: trip.from, to: trip.to, note: trip.note }).catch(
      () => undefined,
    );
  };
  return (
    <InlineText
      value={trip.name}
      placeholder={placeholder}
      label={t("details.name")}
      {...(onChange ? { onChange } : { onCommit: store })}
      look="label"
      placeholderTone={placeholderTone}
      maxLength={DETAIL_MAX.name}
      editable={editable}
      testID="trip-name"
    />
  );
}
