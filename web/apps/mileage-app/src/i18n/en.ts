/**
 * Every sentence the app shows, in one catalogue (docs/spec/mobile.md 6: English first; the label
 * tables of the other six shipped languages are step 6). A new language is another object with the
 * same keys: `Messages` makes the compiler check that none is missing. Placeholders are `{name}`.
 *
 * A sentence that depends on a count has one key per plural form (`name.one`, `name.other`; a
 * language adds the forms it has) and is read with `tp(name, count)`. Numbers and dates are
 * formatted with `Intl` in `index.ts`, not written into a sentence.
 */
export const en = {
  "common.back": "Back",
  "words.error": "Error",
  "words.info": "Note",
  "words.success": "Done",
  "words.warning": "Warning",
  "words.danger": "Problem",
  "common.cancel": "Cancel",
  "common.retry": "Try again",
  "common.close": "Close",
  "common.loading": "Loading",
  "common.unknown": "Unknown",
  "common.details": "Details",
  "error.generic": "Something went wrong. Try again.",
  "error.sync": "The trips could not be sent. The phone keeps them and tries again.",
  "error.database": "The trips on this phone could not be opened.",
  "error.database.hint":
    "Nothing was deleted. Try again. If a trip is being recorded, the recording goes on until you stop it here.",
  "error.database.stop": "Stop the location service",
  "error.database.stopped": "The location service was stopped.",

  "unit.trips.one": "{count} trip",
  "unit.trips.other": "{count} trips",
  "unit.points.one": "{count} point",
  "unit.points.other": "{count} points",
  "unit.minutes.one": "{count} minute",
  "unit.minutes.other": "{count} minutes",
  "unit.km": "{value} km",
  "unit.m": "{value} m",

  "signin.title": "Sign in",
  "signin.intro":
    "Enter the address of your obop instance. You sign in on the instance's own page in your browser; this app never sees your password.",
  "signin.address": "Server address",
  "signin.address.helper": "For example https://obop.example.com",
  "signin.button": "Sign in with your browser",
  "signin.busy": "Signing in",
  "signin.back": "Back to the recording",
  "signin.error.address": "That is not an address. Use the form https://obop.example.com.",
  "signin.error.cleartext":
    "This build connects over https only. Use an address that starts with https.",
  "signin.error.cancelled": "The sign-in was not completed.",
  "signin.error.denied": "Access was refused on the sign-in page.",
  "signin.error.network": "Could not reach the server. Check the address and your connection.",
  "signin.error.server": "That address did not answer like an obop instance.",
  "signin.error.failed": "The sign-in failed. Try again.",
  "signin.expired.title": "Your sign-in has ended",
  "signin.expired.message":
    "Sign in again to upload your trips. The trips on this phone are kept until they are uploaded.",
  "signin.untold.title": "The instance was not told",
  "signin.untold.message":
    "You are signed out on this phone, but the instance could not be reached, so it still lists this phone. Remove it in the back office under Customize, Security, Devices, or it ends by itself when the sign-in runs out.",

  "home.title": "Mileage",
  "home.start": "Start",
  "home.start.hint": "Starts recording a trip",
  "home.purpose": "Purpose of the trip",
  "home.last": "Last trip: {distance}, {state}",
  "home.none": "No trips yet.",
  "home.open.title": "A trip is still open",
  "home.open.message": "Started {when}. Stop it or discard it.",
  "home.open.action": "Open trip",
  "home.waiting.title": "Waiting to upload",
  "home.waiting.message.one":
    "{count} trip is on this phone and not on the server yet. It is safe.",
  "home.waiting.message.other":
    "{count} trips are on this phone and not on the server yet. They are safe.",
  "home.problem.title": "A trip needs your attention",
  "home.problem.message": "The server refused a step. Open the trip to see why.",
  "home.problem.action": "Open trips",
  "home.trips": "Trips",
  "home.account": "Account",

  "purpose.business": "Business",
  "purpose.private": "Private",
  "purpose.commute": "Commute",

  "disclosure.title": "How this app uses your location",
  "disclosure.body":
    "Mileage collects your location to work out how far you drove. It does this only between Start and Stop, and it keeps collecting when the screen is off or the app is in the background, so that the whole trip is recorded. A notification shows while it records. Your route is saved on this phone, encrypted, until your instance has it, and it is not shared with anyone else.",
  "disclosure.continue": "Continue",
  "disclosure.decline": "Not now",
  "permission.denied.title": "Location is needed to record a trip",
  "permission.denied.message":
    "Without access to your location the app cannot measure the trip. You can allow it when you press Start again.",
  "permission.blocked.title": "Location is turned off for this app",
  "permission.blocked.message":
    "Open the settings of the app and allow location, then press Start again.",
  "permission.approximate.title": "Precise location is needed",
  "permission.approximate.message":
    "You allowed only approximate location, and a trip cannot be measured with it. Open the settings of the app, allow Precise location, then press Start again.",
  "permission.services.title": "Location is switched off on this phone",
  "permission.services.message":
    "Switch location on in the quick settings of the phone, then try again.",
  "permission.settings": "Open settings",
  "permission.notifications.title": "Notifications are off",
  "permission.notifications.message":
    "Android will not show that a trip is recording. Recording still works. You can turn notifications on in the settings of the app.",
  "guide.title": "Keep the app running",
  "guide.body":
    "Some phones stop apps to save battery. Do not force-stop the app while you drive. If a trip stops recording, set the battery use of this app to Unrestricted in the Android settings.",
  "guide.start": "Start the trip",

  "recording.title": "Recording",
  "recording.distance":
    "{distance} so far. This is the phone's own count; the server decides the distance.",
  "recording.points.one": "{count} point saved on this phone",
  "recording.points.other": "{count} points saved on this phone",
  "recording.nofix": "Waiting for the first location fix. Go outside or wait a moment.",
  "recording.stop": "Stop",
  "recording.stop.title": "Stop the trip?",
  "recording.stop.message": "The trip ends here and is uploaded.",
  "recording.stop.failed": "The trip could not be stopped. Try again.",
  "recording.stuck.title": "Location updates are not running",
  "recording.stuck.message":
    "The trip is open but the phone is not recording it. Restart recording.",
  "recording.stuck.action": "Restart recording",
  "recording.nofix.title": "No position arrived yet",
  "recording.nofix.message":
    "Nothing was received in the first minute. Check that location is switched on for the phone and that the phone has Google Play services, then restart recording.",
  "recording.quiet.title": "No new position for a while",
  "recording.quiet.message.one":
    "The last position is {count} minute old. If you are standing still that is normal. If you are driving, restart recording.",
  "recording.quiet.message.other":
    "The last position is {count} minutes old. If you are standing still that is normal. If you are driving, restart recording.",
  "recording.services.title": "Location is switched off on this phone",
  "recording.services.message":
    "Switch location on in the quick settings of the phone, then restart recording.",
  "recording.precise.title": "Only approximate location is allowed",
  "recording.precise.message":
    "A trip needs precise location. Allow Precise location in the settings of the app, then restart recording.",
  "recording.permission.title": "Location is not allowed any more",
  "recording.permission.message":
    "Allow location in the settings of the app, then restart recording.",
  "recording.signedout.title": "Your sign-in has ended",
  "recording.signedout.message":
    "The trip keeps recording on this phone and is uploaded after you sign in. You can stop it here.",
  "recording.signedout.action": "Sign in",
  "recording.full.title": "This trip is full",
  "recording.full.message": "A trip holds at most 20,000 points. Stop it and start a new one.",
  "recording.keep.title": "Keep this app running",
  "recording.keep.message": "Do not force-stop it while you drive.",
  "recording.notification.title": "Recording your trip",
  "recording.notification.body": "Mileage is saving your route. Open the app to stop.",
  "recording.error.start": "The trip could not be started. Try again.",

  "announce.recording.started": "Recording started",
  "announce.recording.stopped": "Recording stopped. The trip is saved on this phone.",
  "announce.recording.stuck": "Recording has stopped unexpectedly. Restart recording.",
  "announce.saved": "Saved",

  "trips.title": "Trips",
  "trips.empty": "No trips yet.",
  "trips.offline": "Could not reach the server. Showing the trips saved on this phone.",
  "trips.refresh": "Refresh",
  "trips.refreshing": "Refreshing",

  "state.recording": "Recording",
  "state.uploading": "Uploading",
  "state.waiting": "Waiting to upload",
  "state.ended": "Ended",
  "state.in_review": "In review",
  "state.confirmed": "Confirmed",
  "state.rejected": "Rejected",
  "state.discarded": "Discarded",
  "state.problem": "Needs attention",

  "trip.title": "Trip",
  "trip.started": "Started",
  "trip.ended": "Ended",
  "trip.distance": "Distance",
  "trip.distance.pending": "The server has not counted it yet",
  "trip.purpose": "Purpose",
  "trip.state": "State",
  "trip.checks": "Checks by the server",
  "trip.check.seq_gap.one": "{count} gap in the numbering of the points",
  "trip.check.seq_gap.other": "{count} gaps in the numbering of the points",
  "trip.check.speed_jump.one": "{count} point skipped for an impossible jump",
  "trip.check.speed_jump.other": "{count} points skipped for impossible jumps",
  "trip.check.low_accuracy.one": "{count} point skipped for low accuracy",
  "trip.check.low_accuracy.other": "{count} points skipped for low accuracy",
  "trip.submit": "Submit for review",
  "trip.submit.hint": "Sends the trip to a reviewer",
  "trip.discard": "Discard trip",
  "trip.rejected": "A reviewer rejected this trip: {reason}",
  "trip.waiting": "This trip is not fully uploaded yet. It is safe on this phone.",
  "trip.incomplete.title": "The server is still missing points",
  "trip.incomplete.message":
    "The server has not received every point of this trip yet. The phone keeps trying. If it does not finish, discard the trip and record it again.",
  "trip.problem.title": "The server refused a step",
  "trip.problem.hint": "Nothing is lost. You can try again, or discard the trip.",
  "trip.problem.generic": "The server refused this step.",
  "trip.problem.state_conflict":
    "The trip is in a state where the server does not accept this step.",
  "trip.problem.discarded":
    "The server discarded this trip. Discard it here too, and the points on this phone are removed.",
  "trip.problem.gone":
    "The server does not have this trip. Discard it here, and the points on this phone are removed.",
  "trip.problem.invalid_response": "The server answered in a way the app does not understand.",
  "trip.problem.details": "Details from the server: {message}",
  "trip.action.failed": "That did not work. Try again.",
  "trip.notfound": "This trip is not on this phone any more.",
  "discard.title": "Discard this trip?",
  "discard.message": "A discarded trip never counts. Say why.",
  "discard.reason": "Reason",
  "discard.reason.helper": "Required. A reviewer can read it.",
  "discard.confirm": "Discard",

  "foreign.title": "Trips of another sign-in",
  "foreign.message":
    "This phone still holds {trips}, with {points} of route, that were recorded under another sign-in or on another server. They are not uploaded and not shown, and nothing is deleted until you choose.",
  "foreign.stopped": "A trip of that sign-in was still recording. The recording was stopped.",
  "foreign.keep": "Sign out and keep them",
  "foreign.keep.hint":
    "The person who recorded them can sign in on this phone later and upload them.",
  "foreign.delete": "Delete them and continue",
  "foreign.delete.title": "Delete these trips?",
  "foreign.delete.message":
    "{trips}, with {points} of route, exist only on this phone. They cannot be brought back.",
  "foreign.delete.confirm": "Delete them",

  "account.title": "Account",
  "account.server": "Server",
  "account.device": "This phone",
  "account.device.name": "Device name",
  "account.device.save": "Save name",
  "account.device.saved": "Saved",
  "account.device.revoke": "Remove this phone",
  "account.device.revoke.title": "Remove this phone?",
  "account.device.revoke.message": "The instance ends this sign-in and the app signs out.",
  "account.device.list":
    "Other phones are listed in the back office, under Customize, Security, Devices. An app can only see its own phone.",
  "account.device.error": "Could not read this phone from the server.",
  "account.leave.error":
    "That did not work, and nothing was removed. Check the connection and try again.",
  "account.signout": "Sign out",
  "account.signout.title": "Sign out?",
  "account.signout.message": "You sign in again with your browser.",
  "account.leave.pending.title": "Some trips are not uploaded yet",
  "account.leave.pending.message":
    "{trips}, with {points} of route, exist only on this phone. If you keep them, they are uploaded the next time you sign in to this server with the same account. Nobody who signs in with another account can see or upload them.",
  "account.leave.keep.signout": "Keep them and sign out",
  "account.leave.keep.remove": "Keep them and remove this phone",
  "account.leave.delete": "Delete them",
  "account.leave.delete.title": "Delete these trips?",
  "account.leave.delete.message":
    "{trips}, with {points} of route, are deleted from this phone. This cannot be undone.",
  "account.leave.delete.signout": "Delete them and sign out",
  "account.leave.delete.remove": "Delete them and remove this phone",
  "account.storage": "Storage",
  "account.storage.value": "Encrypted on this phone (SQLCipher {version})",
  "account.version": "App version",
  "account.upload": "Upload",
  "account.upload.idle": "Everything is uploaded",
  "account.upload.pending.one": "{count} trip waiting",
  "account.upload.pending.other": "{count} trips waiting",
  "account.upload.stuck.one": "{count} trip is stuck. Open Trips to see why.",
  "account.upload.stuck.other": "{count} trips are stuck. Open Trips to see why.",
  "account.upload.offline": "No connection, will retry",
  "account.upload.limited": "The server asked to wait, will retry",
  "account.upload.unavailable": "The server is busy, will retry",
  "account.upload.signed_out": "Signed out",
  "account.upload.now": "Upload now",
} as const;

export type MessageKey = keyof typeof en;

/** The shape every language has: all the keys, plain text. */
export type Messages = Record<MessageKey, string>;
