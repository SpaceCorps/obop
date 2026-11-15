// The one catalogue of the native screens: English. Other languages come later; the page inside
// the WebView has its own seven.
export const en = {
  // The few words the components of @obop/ui-native say themselves.
  "common.back": "Back",
  "words.error": "Error",
  "words.info": "Note",
  "words.success": "Done",
  "words.warning": "Warning",
  "words.danger": "Problem",

  // The address screen.
  "address.title": "Connect to your server",
  "address.lead": "Enter the address of your obop server.",
  "address.label": "Server address",
  "address.placeholder": "https://obop.example.com",
  "address.helper":
    "For example https://shop.example.com, or https://shop.example.com/admin when obop runs under a path.",
  "address.connect": "Connect",
  "address.connecting": "Checking the address",
  "address.problem.empty": "Enter the address of your server.",
  "address.problem.scheme": "The address has to start with https://.",
  "address.problem.scheme.test":
    "The address has to start with https://, or with http:// for a server on this device or your local network.",
  "address.problem.extras":
    "Leave out a user name, a password, a question mark, a # and spaces. The address is a host, a port if needed and one path, for example https://shop.example.com/admin.",
  "address.problem.non_ascii":
    "Type the host in plain letters, digits, hyphens and dots. A name with other letters is typed in its xn-- form.",
  "address.problem.host":
    "This is not the name of a host. Check the address, for example https://shop.example.com.",
  "address.problem.path":
    "After the host the address may have one path of lower-case letters, digits, - and _, for example /admin.",
  "address.problem.cleartext":
    "This app connects over https only. An address that starts with http:// is not encrypted.",
  "address.problem.cleartext_public":
    "This build speaks plain http only to this device or your local network. Use https:// for this address.",
  "address.probe.no_answer": "No answer from {host}. Check the address and your connection.",
  "address.probe.not_obop": "{host} answers, but not as an obop server. Check the address.",
  "address.probe.busy":
    "{host} answers with a server error. If it is your obop server, it may be starting or stopped: try again in a moment.",
  "address.probe.forward_cleartext":
    "{host} forwards to an address that is not encrypted. The app does not follow it.",
  "address.forwarded.title": "This address forwards to {origin}.",
  "address.forwarded.message":
    "The app checks that address before it uses it. Go on only if it is your server.",
  "address.forwarded.use": "Use {origin}",
  "address.storage": "The phone could not store the address. Try again.",
  "address.test.title": "Test build",
  "address.test.message":
    "For throwaway servers only. Never sign in to a server with real data with this build.",

  // The host bar.
  "bar.label": "Server {host}",
  "bar.notEncrypted": "not encrypted",
  "bar.testBuild": "test build",
  "bar.newSince": "new since {date}",
  "bar.open": "This app",
  "bar.hint": "Opens the screen about this app and its server.",

  // The cover while the page is not there yet.
  "cover.opening": "Opening {host}",

  // The error screen.
  "error.title": "The server cannot be shown",
  "error.no_answer": "No answer from {host}. Check your connection and try again.",
  "error.tls":
    "The connection to {host} is not secure: its certificate was not accepted. The app does not open it.",
  "error.server_fault": "{host} reports a fault ({status}). Try again in a moment.",
  "error.timeout": "{host} did not answer in {seconds} seconds.",
  "error.forwards": "{host} forwards to {to}.",
  "error.forwards.hint":
    "The app shows one server and does not follow it to another. If {to} is your server, change the server.",
  "error.detail": "What the phone reported: {detail}",
  "error.retry": "Try again",
  "error.changeServer": "Change server",
  "error.thisApp": "This app",

  // "This app".
  "settings.title": "This app",
  "settings.server": "Server",
  "settings.address": "Address",
  "settings.connection": "Connection",
  "settings.connection.https": "Encrypted (https)",
  "settings.connection.http": "Not encrypted (http)",
  "settings.serverVersion": "Server version",
  "settings.added": "Connected since",
  "settings.app": "App",
  "settings.appVersion": "Version",
  "settings.build": "Build",
  "settings.build.plain": "Plain build",
  "settings.build.test": "Test build",
  "settings.cleartext": "Plain http to local addresses",
  "settings.cleartext.on": "Allowed",
  "settings.cleartext.off": "Not allowed",
  "settings.inspectable": "Open to DevTools",
  "settings.inspectable.on": "Yes",
  "settings.inspectable.off": "No",
  "settings.test.message":
    "A test build is for throwaway servers only. Never sign in to a server with real data with it.",
  "settings.openInBrowser": "Open in browser",
  "settings.changeServer": "Change server",

  // The one confirmation before Change server.
  "change.title": "Change server?",
  "change.message":
    "The app forgets {host} and asks for an address again. This does not sign you out on the server: a session there ends by itself, or with Sign out everywhere under Security.",
  "change.confirm": "Change server",
  "change.cancel": "Cancel",
} as const;

export type MessageKey = keyof typeof en;
