import type { FieldMeta } from "./api.ts";
import { optionLabel } from "./format.ts";
import { Badge as UiBadge, Button, Dialog } from "@obop/ui";

// The primitives live in @obop/ui; this file holds the few pieces that know about obop metadata.
export {
  Avatar,
  Button,
  Card,
  Checkbox,
  Chip,
  CodeBlock,
  Dialog,
  EmptyState,
  Icon,
  IconButton,
  Input,
  Kbd,
  Logo,
  Menu,
  Notice,
  Pager,
  Panel,
  Popover,
  Segmented,
  Select,
  Skeleton,
  Spinner,
  Stat,
  Switch,
  Tabs,
  Textarea,
  Toast,
  ToastRegion,
  Tooltip,
  useDismiss,
} from "@obop/ui";
export type { ToastItem } from "@obop/ui";

const OK = new Set([
  "done",
  "won",
  "posted",
  "confirmed",
  "closed",
  "available",
  "running",
  "resolved",
  "active",
  "paid",
]);
const BAD = new Set(["lost", "cancelled", "canceled", "failed", "rejected"]);
const WARN = new Set(["doing", "in_service", "maintenance", "in_progress", "qualified", "pending"]);
const NEUTRAL = new Set(["draft", "new", "open", "todo", "planned", "none", ""]);

/** Status-like words keep a meaning-carrying colour, any other option takes a palette colour. */
export function toneClass(f: FieldMeta | undefined, value: string): string {
  const v = value.toLowerCase();
  if (OK.has(v)) return "t-ok";
  if (BAD.has(v)) return "t-bad";
  if (WARN.has(v)) return "t-warn";
  if (NEUTRAL.has(v)) return "t-neutral";
  const i = f?.options?.indexOf(value) ?? 0;
  return `t-${(Math.max(i, 0) % 6) + 1}`;
}

export function Badge({ field, value }: { field?: FieldMeta; value: string }) {
  return <UiBadge tone={toneClass(field, value) as `t-${string}`}>{optionLabel(value)}</UiBadge>;
}

export type ConfirmRequest = {
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  resolve: (ok: boolean) => void;
};

export function ConfirmDialog({ req }: { req: ConfirmRequest }) {
  return (
    <Dialog
      role="alertdialog"
      size="sm"
      title="Confirm"
      bare
      className="confirm"
      describedBy="confirm-msg"
      onClose={() => req.resolve(false)}
      footer={
        <>
          <Button data-autofocus onClick={() => req.resolve(false)}>
            {req.cancelLabel}
          </Button>
          <Button variant="danger" onClick={() => req.resolve(true)}>
            {req.confirmLabel}
          </Button>
        </>
      }
    >
      <p id="confirm-msg" className="confirm-msg">
        {req.message}
      </p>
    </Dialog>
  );
}
