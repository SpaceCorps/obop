// The generated home of an app (`#/<app>/overview`): lane L1 builds the "what needs me" cards here
// from the aggregate and search commands. A stub for now: the app's name and a link to each of
// its main models, so the address works before the cards exist.
import { t } from "@obop/ui";
import type { App } from "./Chrome.tsx";
import { useData } from "./data.tsx";
import { modelTitle } from "./labels.ts";
import { indexModels, menuOf } from "./presentation.ts";
import { href, type Go, type Route } from "./route.ts";

export type AppOverviewProps = { app: App; route: Route; go: Go };

export function AppOverview({ app }: AppOverviewProps) {
  const d = useData();
  const menu = menuOf(app.module, indexModels(d.modules));
  return (
    <section className="main panel" aria-label={t("{app} overview", { app: app.name })}>
      <header className="home-head">
        <div>
          <h1>{app.name}</h1>
        </div>
      </header>
      <div className="home-body">
        <ul>
          {menu.main.map((m) => (
            <li key={m.name}>
              <a href={href({ app: app.id, model: m.name })}>{modelTitle(m, app.module.id)}</a>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
