import {
  InsetsProvider,
  Screen,
  Text,
  ThemeProvider,
  WordsProvider,
  type Words,
} from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useMemo, useReducer, type ReactNode } from "react";
import { BackHandler } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { t } from "./src/i18n/index.ts";
import type { Runtime } from "./src/native/runtime.ts";
import { useOutbox, useRuntime, useSessionStatus } from "./src/ui/hooks.ts";
import { reduce, type Nav, type Route } from "./src/ui/router.ts";
import { Account } from "./src/ui/screens/Account.tsx";
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
  const { active, loaded } = useOutbox(rt);
  const [stack, dispatch] = useReducer(reduce, [{ name: "home" }] as Route[]);
  const signedIn = status === "signed_in";

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

  if (!signedIn) return <SignIn rt={rt} expired={status === "expired"} />;

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

function Root() {
  const loaded = useRuntime();
  if (loaded.state === "loading") {
    return (
      <Screen title={t("home.title")} scroll={false}>
        <Text tone="muted">{t("common.loading")}</Text>
      </Screen>
    );
  }
  if (loaded.state === "failed") {
    return (
      <Screen title={t("home.title")}>
        <Text tone="danger">{t("error.database", { message: loaded.message })}</Text>
      </Screen>
    );
  }
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
