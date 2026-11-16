import { InsetsProvider, Screen, Text, ThemeProvider } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import type { ReactNode } from "react";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";

/** Hands the space of the system bars to the components of `@obop/ui-native`. */
function Insets({ children }: { children: ReactNode }) {
  const insets = useSafeAreaInsets();
  return <InsetsProvider insets={insets}>{children}</InsetsProvider>;
}

/** The skeleton: one screen. The screens of the prototype replace it (PLAN.md, block D). */
export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <Insets>
          <Screen title="Rutsa">
            <Text tone="muted">Mileage log</Text>
          </Screen>
          <StatusBar style="auto" />
        </Insets>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
