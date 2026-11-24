import { Button, InsetsProvider, Screen, Text, ThemeProvider, useInsets } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useMemo, useReducer, useState, type ReactNode } from "react";
import { BackHandler, LayoutAnimation, View } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import type { TripRepository } from "./src/core/repository.ts";
import { t } from "./src/i18n/index.ts";
import { StatusBanner } from "./src/ui/StatusBanner.tsx";
import { usePhoneStatus, useRecordingCheck, useStore, useTrips } from "./src/ui/hooks.ts";
import { canGoBack, current, reduce, type Nav, type Route } from "./src/ui/router.ts";
import { Failed } from "./src/ui/screens/Failed.tsx";
import { Home } from "./src/ui/screens/Home.tsx";
import { Permissions } from "./src/ui/screens/Permissions.tsx";
import { TripDetail } from "./src/ui/screens/TripDetail.tsx";
import { Trips } from "./src/ui/screens/Trips.tsx";
import { bannerShown, problemsOf, type Problem } from "./src/ui/status.ts";

/** Hands the space of the system bars to the components of `@obop/ui-native`. */
function Insets({ children }: { children: ReactNode }) {
  const insets = useSafeAreaInsets();
  return <InsetsProvider insets={insets}>{children}</InsetsProvider>;
}

function Loading() {
  return (
    <Screen title={t("app.title")} scroll={false}>
      <Text tone="muted">{t("common.loading")}</Text>
    </Screen>
  );
}

/** TEMPORARY: the route whose screen comes in D7; nothing leads to it yet. */
function NotYet({ name, nav }: { name: Route["name"]; nav: Nav }) {
  return (
    <Screen title={name} onBack={nav.back}>
      <Text tone="muted">This screen comes in a later step of block D.</Text>
      <Button label="Home" variant="secondary" onPress={nav.home} />
    </Screen>
  );
}

// The problems the driver closed in the status banner: they stay closed until the next start of
// the app, and a new one shows the banner again (`ui/status.ts`).
const closedProblems = new Set<Problem>();

/**
 * The screens, once the trips could be opened: a small stack (`ui/router.ts`) on the home screen,
 * where a trip is also recorded (D4), the back button of Android, the check of a recording (C5),
 * which runs here whatever screen is open, and the status banner above every screen but the one
 * of the permissions, which says the same in full. While a trip is open the home screen says what
 * keeps it from recording (with its way on), so the banner keeps only the notifications.
 */
function Routes({
  store,
  recoveredStaleFile,
}: {
  store: TripRepository;
  recoveredStaleFile: boolean;
}) {
  const [stack, dispatch] = useReducer(reduce, [{ name: "home" }] as Route[]);
  const { active } = useTrips(store);
  const check = useRecordingCheck();
  const status = usePhoneStatus();
  const insets = useInsets();
  const [, setClosings] = useState(0);

  const nav: Nav = useMemo(
    () => ({
      push: (route) => dispatch({ type: "push", route }),
      replace: (route) => dispatch({ type: "replace", route }),
      back: () => dispatch({ type: "back" }),
      home: () => dispatch({ type: "home" }),
    }),
    [],
  );

  // One screen back while there is one; on the first screen the system leaves the app.
  useEffect(() => {
    const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
      if (!canGoBack(stack)) return false;
      nav.back();
      return true;
    });
    return () => subscription.remove();
  }, [stack, nav]);

  const route = current(stack);
  const all = status === null ? [] : problemsOf(status);
  const problems = active ? all.filter((p) => p === "notifications_off") : all;
  const banner = route.name !== "permissions" && bannerShown(problems, closedProblems);
  const closeBanner = () => {
    // The banner folds up; the screen under it takes the space back.
    LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
    for (const problem of problems) closedProblems.add(problem);
    setClosings((n) => n + 1);
  };

  // The screen over the home screen; `null` on the home screen.
  let screen: ReactNode = null;
  switch (route.name) {
    case "home":
      break;
    case "permissions":
      screen = <Permissions nav={nav} next={route.next} />;
      break;
    case "trips":
      screen = <Trips store={store} nav={nav} />;
      break;
    case "trip":
      screen = <TripDetail store={store} nav={nav} id={route.id} />;
      break;
    case "add":
      screen = <NotYet name={route.name} nav={nav} />;
      break;
  }

  return (
    <View style={{ flex: 1 }}>
      {banner ? <StatusBanner problems={problems} insets={insets} onClose={closeBanner} /> : null}
      <InsetsProvider insets={banner ? { ...insets, top: 0 } : insets}>
        {/* The home screen stays mounted under the others, hidden while one is open, as in a
            navigation stack: coming back to it (from History, or from the permissions after
            Start) finds it as it was, read and drawn, with no flash of empty values. */}
        <View style={{ flex: 1, display: screen === null ? "flex" : "none" }}>
          <Home store={store} nav={nav} recoveredStaleFile={recoveredStaleFile} check={check} />
        </View>
        {screen === null ? null : <View style={{ flex: 1 }}>{screen}</View>}
      </InsetsProvider>
    </View>
  );
}

/** The trips are opened first: Loading while they open, the screen of the failure if they cannot. */
function Root() {
  const loaded = useStore();
  if (loaded.state === "loading") return <Loading />;
  if (loaded.state === "failed") {
    return <Failed message={loaded.message} problem={loaded.problem} retry={loaded.retry} />;
  }
  return <Routes store={loaded.store} recoveredStaleFile={loaded.recoveredStaleFile} />;
}

export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <Insets>
          <Root />
          <StatusBar style="auto" />
        </Insets>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
