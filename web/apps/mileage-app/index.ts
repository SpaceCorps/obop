// The background location task is defined here, before the app starts: expo-task-manager runs it
// when the foreground service delivers a fix, also when no screen exists (src/native/task.ts).
import "./src/native/task.ts";
import { registerRootComponent } from "expo";
import App from "./App.tsx";

registerRootComponent(App);
