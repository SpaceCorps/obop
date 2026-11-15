// The Sign app's additions to the record panel of a request (D92): the "Create signing link"
// button in the action row, the note on who makes links, and the list of links of the request.
import { registerRecordExtension, type RecordExtProps } from "../extensions.ts";
import { SignByLinkNote, SignLinkAction, SignLinks } from "../SignLinks.tsx";
import { REQUEST_MODEL } from "../signLinks.ts";

registerRecordExtension(REQUEST_MODEL, {
  actions: [
    ({ row, fields, busy }: RecordExtProps) => (
      <SignLinkAction row={row} fields={fields} disabled={busy} />
    ),
  ],
  panels: [
    ({ row }: RecordExtProps) => <SignByLinkNote row={row} />,
    ({ row }: RecordExtProps) => <SignLinks request={row} />,
  ],
});
