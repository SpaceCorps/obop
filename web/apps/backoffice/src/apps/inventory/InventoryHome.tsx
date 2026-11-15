// The Inventory home: the generated overview (what needs attention, the lists) with the three
// everyday jobs of a warehouse as its buttons: receive goods, move stock, scrap.
import { useState } from "react";
import { t } from "@obop/ui";
import { AppOverview, type AppOverviewProps } from "../../AppOverview.tsx";
import { useData } from "../../data.tsx";
import { Button } from "../../ui.tsx";
import { MoveDialog } from "./MoveDialog.tsx";
import { MOVE } from "./moves.ts";

type Job = "receipt" | "internal" | "scrap";

export function InventoryHome(p: AppOverviewProps) {
  const d = useData();
  const [job, setJob] = useState<Job | null>(null);
  const canMove = !!d.models.get(MOVE)?.model.can_create;
  const actions = canMove ? (
    <>
      <Button variant="primary" icon="download" onClick={() => setJob("receipt")}>
        {t("Receive goods")}
      </Button>
      <Button className="sup-job" icon="arrow-right" onClick={() => setJob("internal")}>
        {t("Move stock")}
      </Button>
      <Button className="sup-job" icon="trash" onClick={() => setJob("scrap")}>
        {t("Scrap")}
      </Button>
    </>
  ) : undefined;
  return (
    <>
      <AppOverview {...p} actions={actions} />
      {job && <MoveDialog kind={job} onClose={() => setJob(null)} />}
    </>
  );
}
