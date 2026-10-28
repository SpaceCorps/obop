/**
 * Every sentence the app shows, in one catalogue (docs/spec/mobile.md 6: English first; the label
 * tables of the other six shipped languages are step 6). A new language is another object with the
 * same keys: `Messages` makes the compiler check that none is missing. Placeholders are `{name}`.
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
  "error.generic": "Something went wrong. Try again.",
  "error.database": "The trips on this phone could not be opened: {message}",

  "signin.title": "Sign in",
  "signin.intro":
    "Enter the address of your obop instance. You sign in on the instance's own page in your browser; this app never sees your password.",
  "signin.address": "Server address",
  "signin.address.helper": "For example https://obop.example.com",
  "signin.button": "Sign in with your browser",
  "signin.busy": "Signing in",
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
  "home.waiting.message":
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
  "recording.points": "{count} points saved on this phone",
  "recording.nofix": "Waiting for the first location fix. Go outside or wait a moment.",
  "recording.stop": "Stop",
  "recording.stop.title": "Stop the trip?",
  "recording.stop.message": "The trip ends here and is uploaded.",
  "recording.stuck.title": "Location updates are not running",
  "recording.stuck.message":
    "The trip is open but the phone is not recording it. Restart recording.",
  "recording.stuck.action": "Restart recording",
  "recording.full.title": "This trip is full",
  "recording.full.message": "A trip holds at most 20,000 points. Stop it and start a new one.",
  "recording.keep.title": "Keep this app running",
  "recording.keep.message": "Do not force-stop it while you drive.",
  "recording.notification.title": "Recording your trip",
  "recording.notification.body": "Mileage is saving your route. Open the app to stop.",
  "recording.error.start": "The trip could not be started: {message}",

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
  "trip.submit": "Submit for review",
  "trip.submit.hint": "Sends the trip to a reviewer",
  "trip.discard": "Discard trip",
  "trip.rejected": "A reviewer rejected this trip: {reason}",
  "trip.waiting": "This trip is not fully uploaded yet. It is safe on this phone.",
  "trip.problem.title": "The server refused a step",
  "trip.problem.hint": "Nothing is lost. You can try again, or discard the trip.",
  "trip.notfound": "This trip is not on this phone any more.",
  "discard.title": "Discard this trip?",
  "discard.message": "A discarded trip never counts. Say why.",
  "discard.reason": "Reason",
  "discard.reason.helper": "Required. A reviewer can read it.",
  "discard.confirm": "Discard",

  "account.title": "Account",
  "account.server": "Server",
  "account.device": "This phone",
  "account.device.name": "Device name",
  "account.device.save": "Save name",
  "account.device.saved": "Saved",
  "account.device.revoke": "Remove this phone",
  "account.device.revoke.title": "Remove this phone?",
  "account.device.revoke.message":
    "The instance ends this sign-in and the app signs out. Trips on this phone that are not uploaded are deleted.",
  "account.device.list":
    "Other phones are listed in the back office, under Customize, Security, Devices. An app can only see its own phone.",
  "account.device.error": "Could not read this phone from the server.",
  "account.signout": "Sign out",
  "account.signout.title": "Sign out?",
  "account.signout.message": "You sign in again with your browser.",
  "account.signout.pending.title": "Some trips are not uploaded yet",
  "account.signout.pending.message":
    "{count} trips are still on this phone. Signing out now deletes them. Upload them first by staying signed in and online.",
  "account.signout.delete": "Sign out and delete them",
  "account.storage": "Storage",
  "account.storage.value": "Encrypted on this phone (SQLCipher {version})",
  "account.version": "App version",
  "account.upload": "Upload",
  "account.upload.idle": "Everything is uploaded",
  "account.upload.pending": "{count} trips waiting",
  "account.upload.offline": "No connection, will retry",
  "account.upload.limited": "The server asked to wait, will retry",
  "account.upload.unavailable": "The server is busy, will retry",
  "account.upload.signed_out": "Signed out",
  "account.upload.now": "Upload now",
} as const;

export type MessageKey = keyof typeof en;

/** The shape every language has: all the keys, plain text. */
export type Messages = Record<MessageKey, string>;
