import Constants from "expo-constants";

/**
 * A build made for a test instance may carry the address to start from, in the key `defaultServer`
 * of the `extra` of the Expo config (`app.config.ts`). A build that has none, or one that is not
 * text, starts from an empty field.
 */
export const defaultServer = (): string => {
  const value: unknown = Constants.expoConfig?.extra?.defaultServer;
  return typeof value === "string" ? value : "";
};
