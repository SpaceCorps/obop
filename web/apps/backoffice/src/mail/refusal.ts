import { t } from "@obop/ui";

// The mail server guard refuses a password without TLS in English and with the option values
// (`crates/kernel/obop-mail/src/guards.rs`). The form says it with the words it shows.
const NO_PLAIN_PASSWORD = "a password is never sent without TLS";

/** The form's own sentence for the guard's refusal of a password without TLS, else `undefined`. */
export function mailRefusal(raw: string | undefined): string | undefined {
  if (!raw?.includes(NO_PLAIN_PASSWORD)) return undefined;
  const allow = t("Allow a relay on our own network");
  return raw.includes("to turn off")
    ? t(
        "A password is never sent without TLS. To switch off “{allow}”, also set Security to STARTTLS or TLS, or leave the user name empty.",
        { allow },
      )
    : t(
        "A password is never sent without TLS. Set Security to STARTTLS or TLS, leave the user name empty, or switch on “{allow}”.",
        { allow },
      );
}
