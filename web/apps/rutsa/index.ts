// The entry of the app. The background location task will be imported here, before the app is
// registered (PLAN.md, C3): the system can run it when no screen exists.
import { registerRootComponent } from "expo";
import App from "./App.tsx";

registerRootComponent(App);
