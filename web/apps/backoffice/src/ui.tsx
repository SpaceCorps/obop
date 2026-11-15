import type { FieldMeta } from "./api.ts";
import { optionLabel } from "./format.ts";
import { toneClassOf } from "./presentation.ts";
import { Badge as UiBadge, Button, Dialog, t } from "@obop/ui";

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

/**
 * The badge class of a select value: the colour that means something for a status (governed, or
 * named state, stage or status) and for priority words, a neutral palette tint for anything else,
 * so a category never looks like an alarm. One rule for every view: presentation.ts `toneClassOf`.
 */
export function toneClass(f: FieldMeta | undefined, value: string): string {
  return toneClassOf(f, value);
}

export function Badge({ field, value }: { field?: FieldMeta; value: string }) {
  return (
    <UiBadge tone={toneClass(field, value) as `t-${string}`}>{optionLabel(value, field)}</UiBadge>
  );
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
      title={t("Confirm")}
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
