// The background location task is defined here, before the app starts: expo-task-manager runs it
// when the foreground service delivers a fix, also when no screen exists (src/native/task.ts).
import "./src/native/task.ts";
import { registerRootComponent } from "expo";
import App from "./App.tsx";
import { languageStore } from "./src/native/language.ts";

// The language of the first screen is the phone's (set at once); a stored choice follows when it
// has been read from the secure store, which takes a moment. Nothing waits for it.
void languageStore.start();

registerRootComponent(App);
