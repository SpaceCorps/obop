// The background location task is defined here, before the app starts: expo-task-manager runs it
// when the foreground service delivers a fix, also when no screen exists (src/native/task.ts).
import "./src/native/task.ts";
import { registerRootComponent } from "expo";
import App from "./App.tsx";
import { startup } from "./config/startup.ts";
import { startHost } from "./src/native/host.ts";
import { languageStore } from "./src/native/language.ts";

// The host is given the configuration of the app here, at the top level of the entry. Nothing that
// needs it reads it while it is being imported (the task above is imported first), and the system
// calls the task only after this file has run to its end.
startHost(startup);

// The language of the first screen is the phone's (set at once); a stored choice follows when it
// has been read from the secure store, which takes a moment. Nothing waits for it.
void languageStore.start();

registerRootComponent(App);
