import { InsetsProvider, Screen, Text, ThemeProvider, useInsets } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { memo, useEffect, useMemo, useReducer, useRef, useState, type ReactNode } from "react";
import { BackHandler, Keyboard, LayoutAnimation, View, type HostInstance } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import type { TripRepository } from "./src/core/repository.ts";
import { t } from "./src/i18n/index.ts";
import { useRecordPanel } from "./src/ui/RecordPanel.tsx";
import { StatusBanner } from "./src/ui/StatusBanner.tsx";
import { TabBar, TAB_BAR_ROOM } from "./src/ui/TabBar.tsx";
import { usePhoneStatus, useRecordingCheck, useStore, useTrips } from "./src/ui/hooks.ts";
import { backPress, current, reduce, type Nav, type Route, type Tab } from "./src/ui/router.ts";
import { AddTrip as AddTripScreen } from "./src/ui/screens/AddTrip.tsx";
import { Failed } from "./src/ui/screens/Failed.tsx";
import { Home } from "./src/ui/screens/Home.tsx";
import { Permissions as PermissionsScreen } from "./src/ui/screens/Permissions.tsx";
import { Profile as ProfileScreen } from "./src/ui/screens/Profile.tsx";
import { TripDetail as TripDetailScreen } from "./src/ui/screens/TripDetail.tsx";
import { Trips as TripsScreen } from "./src/ui/screens/Trips.tsx";
import { bannerShown, problemsOf, type Problem } from "./src/ui/status.ts";
import { useThemeChoice } from "./src/ui/theme-choice.ts";

// The screens are drawn again only when what they are given changes: the record widget above them
// (`useRecordPanel`) draws `Routes` again every second while a trip records.
const AddTrip = memo(AddTripScreen);
const Permissions = memo(PermissionsScreen);
const Profile = memo(ProfileScreen);
const TripDetail = memo(TripDetailScreen);
const Trips = memo(TripsScreen);

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

// The problems the driver closed in the status banner: they stay closed until the next start of
// the app, and a new one shows the banner again (`ui/status.ts`).
const closedProblems = new Set<Problem>();

/**
 * The screens, once the trips could be opened: the three tabs of the switcher at the bottom
 * (History, Record in the middle, the home screen, D4, and Profile), the switcher holding the
 * record widget, where a trip is recorded, on all three (`useRecordPanel`), a small stack
 * (`ui/router.ts`) of the screens pushed over them without the switcher (a trip, Manual add, the
 * permissions), the back button of Android, the check of a recording (C5), which runs here
 * whatever screen is open, and the status banner above every screen but the one of the
 * permissions, which says the same in full. While a trip is open the home screen says what keeps
 * it from recording (with its way on), so the banner keeps only the notifications.
 */
function Routes({
  store,
  recoveredStaleFile,
}: {
  store: TripRepository;
  recoveredStaleFile: boolean;
}) {
  const [stack, dispatch] = useReducer(reduce, [{ name: "home" }] as Route[]);
  const [tab, setTab] = useState<Tab>("record");
  // A tab is drawn once it was shown, and then kept, so coming back to it finds it as it was.
  const [opened, setOpened] = useState<ReadonlySet<Tab>>(() => new Set<Tab>(["record"]));
  const { active } = useTrips(store);
  const check = useRecordingCheck();
  const status = usePhoneStatus();
  const insets = useInsets();
  const [, setClosings] = useState(0);
  // The switcher with the record widget, the same on the three tabs; the recent trips of Record end
  // above it, and every tab keeps room at its bottom as high as it is.
  const footerRef = useRef<HostInstance>(null);
  const [barRoom, setBarRoom] = useState(TAB_BAR_ROOM);

  const nav: Nav = useMemo(() => {
    const show = (next: Tab) => {
      setTab(next);
      setOpened((seen) => (seen.has(next) ? seen : new Set([...seen, next])));
    };
    return {
      push: (route) => dispatch({ type: "push", route }),
      replace: (route) => dispatch({ type: "replace", route }),
      back: () => dispatch({ type: "back" }),
      home: () => dispatch({ type: "home" }),
      tab: (next) => {
        dispatch({ type: "home" });
        show(next);
      },
    };
  }, []);
  const route = current(stack);
  // On the screen of the trip being recorded the switcher keeps only its buttons: the screen shows
  // the rest.
  const onOpenTrip =
    route.name === "trip" && active !== null && active.id === route.id && active.state !== "ended";
  const recording = useRecordPanel({ store, nav, check, compact: onOpenTrip });
  const keyboard = useKeyboardShown();

  // One screen back while there is one; Profile and History go to Record; on Record the system
  // leaves the app.
  useEffect(() => {
    const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
      switch (backPress(stack, tab)) {
        case "back":
          nav.back();
          return true;
        case "record":
          nav.tab("record");
          return true;
        case "leave":
          return false;
      }
    });
    return () => subscription.remove();
  }, [stack, tab, nav]);

  const all = status === null ? [] : problemsOf(status);
  const problems = active ? all.filter((p) => p === "notifications_off") : all;
  const banner = route.name !== "permissions" && bannerShown(problems, closedProblems);
  // Under the status banner the screens have no space of the status bar to keep.
  const shown = banner ? { ...insets, top: 0 } : insets;
  const closeBanner = () => {
    // The banner folds up; the screen under it takes the space back.
    LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut);
    for (const problem of problems) closedProblems.add(problem);
    setClosings((n) => n + 1);
  };

  // The screen over the tabs; `null` while a tab is shown.
  let screen: ReactNode = null;
  switch (route.name) {
    case "home":
      break;
    case "permissions":
      screen = <Permissions nav={nav} next={route.next} />;
      break;
    case "trip":
      screen = <TripDetail store={store} nav={nav} id={route.id} />;
      break;
    case "add":
      screen = <AddTrip store={store} nav={nav} />;
      break;
  }

  // Over the tabs the keyboard would lift the switcher over the field being typed: it goes while
  // the keyboard is up there (on the tabs the name in the switcher is typed with it).
  const barShown = screen === null || !keyboard;

  return (
    <View style={{ flex: 1 }}>
      {banner ? <StatusBanner problems={problems} insets={insets} onClose={closeBanner} /> : null}
      <InsetsProvider insets={shown}>
        {/* The tabs stay mounted under the screens over them, hidden while one is open, as in a
            navigation stack: coming back to them (from a trip, or from the permissions after
            Start) finds them as they were, read and drawn, with no flash of empty values. They
            keep room at their bottom for the switcher, which lies over them. */}
        <InsetsProvider insets={{ ...shown, bottom: shown.bottom + (barShown ? barRoom : 0) }}>
          <View style={{ flex: 1, display: screen === null ? "flex" : "none" }}>
            <View style={{ flex: 1, display: tab === "record" ? "flex" : "none" }}>
              <Home
                store={store}
                nav={nav}
                recoveredStaleFile={recoveredStaleFile}
                warning={recording.warning}
                restart={recording.restart}
                footerRef={footerRef}
              />
            </View>
            {opened.has("history") ? (
              <View style={{ flex: 1, display: tab === "history" ? "flex" : "none" }}>
                <Trips store={store} nav={nav} />
              </View>
            ) : null}
            {opened.has("profile") ? (
              <View style={{ flex: 1, display: tab === "profile" ? "flex" : "none" }}>
                <Profile />
              </View>
            ) : null}
          </View>
          {screen === null ? null : <View style={{ flex: 1 }}>{screen}</View>}
        </InsetsProvider>
        {/* The switcher with the record widget over its tabs, on every screen: a tab marked on
          the tabs, none on a screen over them. */}
        {barShown ? (
          <TabBar
            tab={screen === null ? tab : null}
            onTab={nav.tab}
            bottom={shown.bottom}
            header={recording.widget}
            barRef={footerRef}
            onRoom={(room) => setBarRoom((was) => (Math.abs(was - room) < 0.5 ? was : room))}
          />
        ) : null}
      </InsetsProvider>
    </View>
  );
}

/** Whether the keyboard is up. */
function useKeyboardShown(): boolean {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    const up = Keyboard.addListener("keyboardDidShow", () => setShown(true));
    const down = Keyboard.addListener("keyboardDidHide", () => setShown(false));
    return () => {
      up.remove();
      down.remove();
    };
  }, []);
  return shown;
}

/** The trips are opened first: Loading while they open, the screen of the failure if they cannot. */
function Root() {
  const loaded = useStore();
  if (loaded.state === "loading") return <Loading />;
  if (loaded.state === "failed") {
    return <Failed message={loaded.message} problem={loaded.problem} retry={loaded.retry} />;
  }
  return (
    <ChosenTheme>
      <Routes store={loaded.store} recoveredStaleFile={loaded.recoveredStaleFile} />
    </ChosenTheme>
  );
}

/**
 * The look of the app once the trips are open: dark, unless light is chosen (in Profile, stage 2;
 * the choice "system" is dark too for now). Loading and the screen of a failure are dark.
 */
function ChosenTheme({ children }: { children: ReactNode }) {
  const [choice] = useThemeChoice();
  // Dark unless light is chosen (stage 1 has no choice yet, so it is dark).
  const mode = choice === "light" ? "light" : "dark";
  return (
    <ThemeProvider mode={mode}>
      {children}
      <StatusBar style={mode === "dark" ? "light" : "dark"} />
    </ThemeProvider>
  );
}

export default function App() {
  return (
    <SafeAreaProvider>
      {/* Dark, as the screens once the trips are open (`ChosenTheme`): Loading and a failure too. */}
      <ThemeProvider mode="dark">
        <Insets>
          <Root />
          <StatusBar style="light" />
        </Insets>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
