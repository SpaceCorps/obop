// What the sign-in page explains inside the phone host app. The app cannot run the round trip to
// Microsoft yet, so the button is not drawn there; where the instance has that sign-in on, one
// sentence says so and names the browser. In a browser this draws nothing.
//
// The page does not say which server it is: a look-alike server would write the same line. The
// app's own bar over the page says it.
import { t } from "@obop/ui";
import { useHost } from "./useHost.ts";

export function SignInNotes({
  microsoft,
}: {
  /** An administrator turned the sign-in with Microsoft on for this instance. */
  microsoft: boolean;
}) {
  const host = useHost();
  if (!host.embedded || !microsoft || host.allows("microsoft")) return null;
  return (
    <p className="hint" role="note">
      {t(
        "Sign-in with Microsoft is not available in this app yet. To sign in that way, use your browser.",
      )}
    </p>
  );
}
