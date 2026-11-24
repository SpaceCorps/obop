import { Button, InsetsProvider, Screen, Text, ThemeProvider } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useMemo, useReducer, type ReactNode } from "react";
import { BackHandler } from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { t } from "./src/i18n/index.ts";
import { useRecordingCheck, useStore } from "./src/ui/hooks.ts";
import { canGoBack, current, reduce, type Nav, type Route } from "./src/ui/router.ts";
import { Failed } from "./src/ui/screens/Failed.tsx";
import { Panel } from "./src/ui/screens/Panel.tsx";
import { Permissions } from "./src/ui/screens/Permissions.tsx";

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

/** TEMPORARY: the routes whose screens come in D4 to D7; nothing leads to them yet. */
function NotYet({ name, nav }: { name: Route["name"]; nav: Nav }) {
  return (
    <Screen title={name} onBack={nav.back}>
      <Text tone="muted">This screen comes in a later step of block D.</Text>
      <Button label="Home" variant="secondary" onPress={nav.home} />
    </Screen>
  );
}

/**
 * The screens, once the trips could be opened: a small stack (`ui/router.ts`), the back button of
 * Android, and the check of a recording (C5), which runs here, whatever screen is open.
 */
function Routes() {
  const [stack, dispatch] = useReducer(reduce, [{ name: "home" }] as Route[]);
  const check = useRecordingCheck();

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
  switch (route.name) {
    case "home":
      return (
        <Panel check={check} onStart={() => nav.push({ name: "permissions", next: "start" })} />
      );
    case "permissions":
      return <Permissions nav={nav} next={route.next} />;
    case "recording":
    case "trips":
    case "trip":
    case "add":
      return <NotYet name={route.name} nav={nav} />;
  }
}

/** The trips are opened first: Loading while they open, the screen of the failure if they cannot. */
function Root() {
  const loaded = useStore();
  if (loaded.state === "loading") return <Loading />;
  if (loaded.state === "failed") {
    return <Failed message={loaded.message} problem={loaded.problem} retry={loaded.retry} />;
  }
  return <Routes />;
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
