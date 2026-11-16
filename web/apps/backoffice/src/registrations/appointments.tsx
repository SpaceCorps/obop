// Appointments: the free times of the chosen resource and day inside the booking form
// (apps/appointments/SlotPicker.tsx, UI plan 3.5).
import { registerRecordExtension } from "../extensions.ts";
import { SlotPicker } from "../apps/appointments/SlotPicker.tsx";
import { APPOINTMENT } from "../apps/appointments/slots.ts";

registerRecordExtension(APPOINTMENT, { sections: [SlotPicker] });
