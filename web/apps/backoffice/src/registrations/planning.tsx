// Planning: Shifts open on the week plan (apps/planning/WeekBoard.tsx: a row per employee and one
// for open shifts, a column per day, and "Publish week"); the calendar and the list stay one click
// away, and a shift opens as a full page. A new shift from a cell starts on its employee and day.
import { lazy } from "react";
import { Button, t } from "@obop/ui";
import { registerModelPage, type ModelPageProps } from "../extensions.ts";
import { ModelPage } from "../ModelPage.tsx";
import { SHIFT, WEEK, takeNewShift } from "../apps/planning/links.ts";

const WeekBoard = lazy(() =>
  import("../apps/planning/WeekBoard.tsx").then((m) => ({ default: m.WeekBoard })),
);

function ShiftsPage(p: ModelPageProps) {
  if (!p.route.id && (p.route.view === "" || p.route.view === WEEK)) return <WeekBoard {...p} />;
  return (
    <ModelPage
      {...p}
      takeNewDefaults={takeNewShift}
      extra={
        <Button icon="columns" onClick={() => p.go({ app: p.app.id, model: SHIFT, view: WEEK })}>
          {t("Week plan")}
        </Button>
      }
    />
  );
}

registerModelPage(SHIFT, ShiftsPage);
