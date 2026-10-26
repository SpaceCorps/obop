// The control plane answers in English. The known messages (docs/DEPLOY.md, the platform crate) are
// mapped to translatable ones; anything else is shown as received.
import { getLang, t } from "@obop/ui";

/**
 * The code words of a refused database call (D74), in the person's language. The server sends
 * `storage_busy` as the error of a 503: a code, not a sentence, so English shows a sentence too.
 */
const STORAGE_FAULTS: Record<string, () => string> = {
  storage_busy: () => t("The server is busy. Try again in a moment."),
  storage_unavailable: () => t("The database cannot be reached right now. Try again in a moment."),
  storage_timeout: () =>
    t("The database took too long to answer. Try again, or ask for less at once."),
};

type Rule = [RegExp, (m: RegExpExecArray) => string];

const RULES: Rule[] = [
  [/^enter a valid email address$/, () => t("Enter a valid email address")],
  [
    /^name must be 3 to 30 letters, digits or hyphens$/,
    () => t("The name must be 3 to 30 letters, digits or hyphens"),
  ],
  [/^the name (\S+) is taken$/, (m) => t("The name {name} is taken", { name: m[1]! })],
  [
    /^an account can have (\d+) instances$/,
    (m) => t("An account can have {count} instances", { count: Number(m[1]) }),
  ],
  [
    /^password must be at least 12 characters$/,
    () => t("The password must be at least 12 characters"),
  ],
  [/^user (.+) exists$/, () => t("An account with this email already exists")],
  [/^invalid username or password$/, () => t("Invalid email or password")],
  [
    /^too many attempts, try again in a minute$/,
    () => t("Too many attempts. Try again in a minute."),
  ],
  [/^not signed in$/, () => t("You are not signed in")],
  [/^no such instance$/, () => t("There is no such instance")],
  [/^wait until provisioning finishes$/, () => t("Wait until the setup has finished")],
  [/^the service did not come up in time$/, () => t("The service did not come up in time")],
  [/^the instance did not start listening$/, () => t("The instance did not start")],
];

export function serverMessage(raw: string | undefined | null): string {
  if (!raw) return "";
  const fault = STORAGE_FAULTS[raw];
  if (fault) return fault();
  if (getLang() === "en") return raw;
  for (const [re, fn] of RULES) {
    const m = re.exec(raw);
    if (m) return fn(m);
  }
  return raw;
}

const clock = (m: string, s: string) => t("{minutes}m {seconds}s", { minutes: m, seconds: s });

/** What the background job reports (English text from the server), in the person's language. */
export function phaseText(raw: string): string {
  const timed =
    /^(Building and starting the service|Waiting for the address to resolve) \((\d+)m (\d+)s\)$/.exec(
      raw,
    );
  if (timed) {
    const time = clock(timed[2]!, timed[3]!);
    return timed[1]!.startsWith("Building")
      ? t("Building and starting the service ({time})", { time })
      : t("Waiting for the address to resolve ({time})", { time });
  }
  switch (raw) {
    case "Queued":
      return t("Queued");
    case "Deleting":
      return t("Deleting");
    case "Creating the database":
      return t("Creating the database");
    case "Creating your admin account":
      return t("Creating your admin account");
    case "Creating the service":
      return t("Creating the service");
    case "Creating the instance":
      return t("Creating the instance");
    case "Starting the server":
      return t("Starting the server");
    case "Removing the service":
      return t("Removing the service");
    case "Removing the database":
      return t("Removing the database");
    case "Removing the instance":
      return t("Removing the instance");
    default:
      return raw;
  }
}

export { clock };
