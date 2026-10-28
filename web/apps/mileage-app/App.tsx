import { createObopClient } from "@obop/client";
import { distanceMeters } from "@obop/trip-core";
import { StatusBar } from "expo-status-bar";
import { StyleSheet, Text, View } from "react-native";

// The typed client of the instance (generated from its OpenAPI document). Nothing calls it yet: the
// next steps sign in and upload with it. It is imported here so that Metro bundles the package.
export const makeClient = createObopClient;

// Synthetic reference pair; the spike only proves that the shared package is bundled.
const FROM = { lat: 50.0, lon: 10.0 };
const TO = { lat: 50.01, lon: 10.01 };

export default function App() {
  const meters = Math.round(distanceMeters(FROM, TO));
  return (
    <View style={styles.screen}>
      <Text style={styles.title}>obop Mileage</Text>
      <Text style={styles.label}>Reference distance (synthetic)</Text>
      <Text style={styles.value} testID="reference-distance">
        {meters} m
      </Text>
      <StatusBar style="auto" />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, alignItems: "center", justifyContent: "center", backgroundColor: "#ffffff" },
  title: { fontSize: 28, fontWeight: "700", color: "#111111" },
  label: { marginTop: 24, fontSize: 14, color: "#555555" },
  value: { marginTop: 4, fontSize: 40, fontWeight: "600", color: "#111111" },
});
