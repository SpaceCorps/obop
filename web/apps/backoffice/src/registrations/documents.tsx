// The Documents app and the Google Drive connected to the instance (D60): a "Google Drive" page in
// the sidebar while a Drive is connected, and on the Documents list the buttons to import a file
// from it (or to connect one), which open the new record form with the file's values.
import { useState } from "react";
import { Button, t } from "@obop/ui";
import { useData } from "../data.tsx";
import {
  DriveBrowser,
  DriveLibrary,
  setPendingDriveDefaults,
  takePendingDriveDefaults,
  useDriveStatus,
} from "../DriveBrowser.tsx";
import { registerAppPages, registerModelPage, type ModelPageProps } from "../extensions.ts";
import { ModelPage } from "../ModelPage.tsx";
import { href } from "../route.ts";
import type { App } from "../Chrome.tsx";

/** The route name of the Google Drive page inside the Documents app. */
const DRIVE = "drive";
const DOCUMENT = "documents_document";
/** What a file brought in from the Drive starts as. */
const FROM_DRIVE = { kind: "contract", confidentiality: "internal" };

/** `documents_document.attachment` when the app's documents have that file field, else "". */
function driveFieldOf(app: App): string {
  return app.module.models.some(
    (m) =>
      m.name === DOCUMENT && m.fields.some((f) => f.name === "attachment" && f.kind === "file"),
  )
    ? `${DOCUMENT}.attachment`
    : "";
}

registerAppPages("documents", {
  use: (ctx) => {
    const driveField = ctx ? driveFieldOf(ctx.app) : "";
    const { may } = useDriveStatus(ctx?.token ?? "", !!driveField && !!ctx?.ready);
    if (!ctx || !driveField || !may) return {};
    const onDrive = ctx.route.model === DRIVE;
    return {
      navAfter: [
        {
          key: DRIVE,
          label: t("Google Drive"),
          href: href({ app: ctx.app.id, model: DRIVE }),
          title: t("The files and folders of the Drive connected to this instance."),
          current: onDrive,
        },
      ],
      page: onDrive ? (
        <DriveLibrary
          token={ctx.token}
          field={driveField}
          onPicked={(v) => {
            setPendingDriveDefaults(v);
            ctx.go({ app: "documents", model: DOCUMENT, id: "new" });
          }}
        />
      ) : null,
    };
  },
});

/** The Documents list, with the buttons that bring a file in from the connected Google Drive. */
function DocumentsPage(p: ModelPageProps) {
  const d = useData();
  const [drive, setDrive] = useState(false);
  const driveField = p.model.fields.some((f) => f.name === "attachment" && f.kind === "file")
    ? `${p.model.name}.attachment`
    : "";
  const { may, canSetUp } = useDriveStatus(d.token, !!driveField);
  return (
    <ModelPage
      {...p}
      // A file imported on the Drive page opens the new record form with its values.
      takeNewDefaults={() => {
        const v = takePendingDriveDefaults();
        return v ? { ...v, ...FROM_DRIVE } : null;
      }}
      extra={
        <>
          {may && driveField && (
            <Button icon="documents" onClick={() => setDrive(true)}>
              {t("From Google Drive")}
            </Button>
          )}
          {canSetUp && driveField && (
            <Button
              icon="documents"
              onClick={() => p.go({ app: "connectors", model: "google_drive" })}
            >
              {t("Connect Google Drive")}
            </Button>
          )}
        </>
      }
      after={(page) =>
        drive && (
          <DriveBrowser
            token={d.token}
            field={driveField}
            onClose={() => setDrive(false)}
            onPicked={(v) => {
              setDrive(false);
              page.openNew({ ...v, ...FROM_DRIVE });
            }}
          />
        )
      }
    />
  );
}

registerModelPage(DOCUMENT, DocumentsPage);
