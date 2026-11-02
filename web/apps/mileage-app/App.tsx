import {
  Button,
  InsetsProvider,
  Screen,
  Text,
  ThemeProvider,
  WordsProvider,
  type Words,
} from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useMemo, useReducer, useState, type ReactNode } from "react";
import { BackHandler } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { t } from "./src/i18n/index.ts";
import { stopRecording } from "./src/native/recorder.ts";
import type { Runtime } from "./src/native/runtime.ts";
import { useBinding, useOutbox, useRuntime, useSessionStatus } from "./src/ui/hooks.ts";
import { tOs } from "./src/ui/os-text.ts";
import { gate, keepSigningIn, reduce, type Nav, type Route } from "./src/ui/router.ts";
import { Account } from "./src/ui/screens/Account.tsx";
import { Foreign } from "./src/ui/screens/Foreign.tsx";
import { Home } from "./src/ui/screens/Home.tsx";
import { Permissions } from "./src/ui/screens/Permissions.tsx";
import { Recording } from "./src/ui/screens/Recording.tsx";
import { SignIn } from "./src/ui/screens/SignIn.tsx";
import { TripDetail } from "./src/ui/screens/TripDetail.tsx";
import { Trips } from "./src/ui/screens/Trips.tsx";

const WORDS: Words = {
  back: t("common.back"),
  error: t("words.error"),
  info: t("words.info"),
  success: t("words.success"),
  warning: t("words.warning"),
  danger: t("words.danger"),
};

function Insets({ children }: { children: ReactNode }) {
  const insets = useSafeAreaInsets();
  return <InsetsProvider insets={insets}>{children}</InsetsProvider>;
}

function Routes({ rt }: { rt: Runtime }) {
  const status = useSessionStatus(rt);
  const binding = useBinding(rt);
  const { active, loaded } = useOutbox(rt);
  const [stack, dispatch] = useReducer(reduce, [{ name: "home" }] as Route[]);
  // While a trip is open and nobody is signed in, the recording (and its Stop button) comes first;
  // the driver goes to the sign-in from there.
  const [signingIn, setSigningIn] = useState(false);
  const signedIn = status === "signed_in";
  // A finished sign-in ends the choice: the next end of a sign-in during a trip shows the recording.
  useEffect(() => {
    setSigningIn((current) => keepSigningIn(current, status));
  }, [status]);

  const nav: Nav = useMemo(
    () => ({
      push: (route) => dispatch({ type: "push", route }),
      replace: (route) => dispatch({ type: "replace", route }),
      back: () => dispatch({ type: "back" }),
      home: () => dispatch({ type: "home" }),
    }),
    [],
  );

  // The system back button goes back one screen; on the first screen it leaves the app.
  useEffect(() => {
    const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
      if (stack.length > 1) {
        nav.back();
        return true;
      }
      return false;
    });
    return () => subscription.remove();
  }, [stack.length, nav]);

  // A trip that was open when the app started is shown first, once the outbox has been read.
  const hadOpenTrip = loaded && active !== null;
  useEffect(() => {
    if (signedIn && hadOpenTrip) dispatch({ type: "push", route: { name: "recording" } });
    // Only when the outbox has been read for the first time, not at every change of the trip.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded, signedIn]);

  const where = gate({
    status,
    foreign: binding.kind === "foreign",
    openTrip: active !== null,
    loaded,
    signingIn,
  });
  if (where === "loading") return <Loading />;
  if (where === "recording") {
    return <Recording rt={rt} nav={nav} signedOut onSignIn={() => setSigningIn(true)} />;
  }
  if (where === "sign_in") {
    return (
      <SignIn
        rt={rt}
        expired={status === "expired"}
        {...(active !== null ? { onBack: () => setSigningIn(false) } : {})}
      />
    );
  }
  // Trips of another sign-in are on the phone: nothing is uploaded or shown until the driver chooses.
  if (where === "foreign" && binding.kind === "foreign") {
    return <Foreign rt={rt} binding={binding} />;
  }

  const route = stack[stack.length - 1]!;
  switch (route.name) {
    case "home":
      return <Home rt={rt} nav={nav} />;
    case "trips":
      return <Trips rt={rt} nav={nav} />;
    case "trip":
      return <TripDetail rt={rt} nav={nav} id={route.id} />;
    case "account":
      return <Account rt={rt} nav={nav} />;
    case "recording":
      return <Recording rt={rt} nav={nav} />;
    case "permissions":
      return <Permissions rt={rt} nav={nav} purpose={route.purpose} />;
  }
}

function Loading() {
  return (
    <Screen title={t("home.title")} scroll={false}>
      <Text tone="muted">{t("common.loading")}</Text>
    </Screen>
  );
}

/**
 * The trips could not be opened (a wrong key, a damaged file). Nothing is deleted. The driver can
 * try again and can stop the location service, which keeps running without the app's screens.
 */
function Failed({ message, retry }: { message: string; retry: () => void }) {
  const [stopped, setStopped] = useState(false);
  const [details, setDetails] = useState(false);
  return (
    <Screen
      title={t("home.title")}
      footer={
        <>
          <Button label={t("common.retry")} onPress={retry} testID="database-retry" />
          <Button
            label={t("error.database.stop")}
            variant="secondary"
            onPress={() =>
              void stopRecording().then(
                () => setStopped(true),
                () => undefined,
              )
            }
            testID="database-stop"
          />
        </>
      }
    >
      <Text tone="danger" testID="database-error">
        {t("error.database")}
      </Text>
      <Text>{tOs("error.database.hint")}</Text>
      {stopped ? <Text tone="muted">{t("error.database.stopped")}</Text> : null}
      <Button label={t("common.details")} variant="ghost" onPress={() => setDetails((d) => !d)} />
      {details ? <Text tone="muted">{message}</Text> : null}
    </Screen>
  );
}

function Root() {
  const loaded = useRuntime();
  if (loaded.state === "loading") return <Loading />;
  if (loaded.state === "failed") return <Failed message={loaded.message} retry={loaded.retry} />;
  return <Routes rt={loaded.runtime} />;
}

export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <WordsProvider words={WORDS}>
          <Insets>
            <Root />
            <StatusBar style="auto" />
          </Insets>
        </WordsProvider>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
