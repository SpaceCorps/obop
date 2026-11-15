import Constants from "expo-constants";
import { readBuild, type Build } from "../core/build.ts";

/**
 * The flags of this build: what `app.config.ts` wrote into `extra` of the Expo config when the
 * app was built (`src/core/build.ts` reads them without trusting their shape).
 */
export const build: Build = readBuild(Constants.expoConfig?.extra);
