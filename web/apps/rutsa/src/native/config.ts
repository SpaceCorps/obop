/**
 * The name the background location task is registered under (`native/task.ts`, `native/recorder.ts`).
 * The system keeps the registration across an update of the app, so the name stays as it is: a new
 * name would leave the old registration with no code to run.
 *
 * After `mileage-app` (`src/native/config.ts`), which also holds the settings of its server.
 */
export const LOCATION_TASK = "org.obop.rutsa.location";
