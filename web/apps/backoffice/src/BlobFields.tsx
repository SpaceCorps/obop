import { useEffect, useId, useRef, useState } from "react";
import { t } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import {
  FILE_ACCEPT,
  IMAGE_ACCEPT,
  IMAGE_TYPES,
  blobUrl,
  formatBytes,
  saveBlob,
  thumbWidth,
  uploadBlob,
  type Upload,
} from "./blobs.ts";
import { useData } from "./data.tsx";
import { fieldTitle } from "./labels.ts";
import { Icon } from "./ui.tsx";

/** True once the element has come near the viewport, so a long list fetches only what it shows. */
function useVisible<T extends Element>() {
  const ref = useRef<T>(null);
  const [seen, setSeen] = useState(typeof IntersectionObserver === "undefined");
  useEffect(() => {
    const el = ref.current;
    if (seen || !el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setSeen(true);
          io.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [seen]);
  return [ref, seen] as const;
}

/** A stored picture, fetched with the session token. `size` is the box in CSS pixels. */
export function BlobThumb({
  id,
  alt,
  size = 40,
  className = "",
}: {
  id: string;
  alt: string;
  size?: number;
  className?: string;
}) {
  const d = useData();
  const [ref, seen] = useVisible<HTMLSpanElement>();
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!seen) return;
    let live = true;
    setFailed(false);
    void blobUrl(d.token, id, thumbWidth(size)).then((u) => {
      if (!live) return;
      if (u) setSrc(u);
      else setFailed(true);
    });
    return () => {
      live = false;
    };
  }, [seen, d.token, id, size]);
  return (
    <span
      ref={ref}
      className={`blobthumb ${className}`}
      style={{ inlineSize: size, blockSize: size }}
      data-state={src ? "ready" : failed ? "failed" : "loading"}
    >
      {src ? (
        <img src={src} alt={alt} loading="lazy" decoding="async" />
      ) : (
        <span
          className="blobthumb-ph"
          role={failed ? "img" : undefined}
          aria-label={failed ? alt : undefined}
        >
          <Icon name={failed ? "warning" : "eye"} size={Math.max(12, size / 3)} />
        </span>
      )}
    </span>
  );
}

/** A file in a list or a card: a button that downloads it. */
export function FileChip({ id, field }: { id: string; field: FieldMeta }) {
  const d = useData();
  const [state, setState] = useState<"idle" | "busy" | "failed">("idle");
  return (
    <button
      type="button"
      className="filechip"
      disabled={state === "busy"}
      title={t("Download {field}", { field: fieldTitle(field) })}
      onClick={async (e) => {
        e.stopPropagation();
        setState("busy");
        const ok = await saveBlob(d.token, id, fieldTitle(field));
        setState(ok ? "idle" : "failed");
        if (!ok) d.notify(t("Could not download the file."), "error");
      }}
    >
      <Icon name="download" size={14} /> {state === "failed" ? t("Try again") : t("Download")}
    </button>
  );
}

type BlobInputProps = {
  field: FieldMeta;
  value: string;
  onChange: (v: string) => void;
  readOnly?: boolean;
  id?: string;
  invalid?: boolean;
  describedBy?: string;
  /** When the model has a `<name>_alt` field: its title, and whether it is still empty. */
  altHint?: { title: string; empty: boolean };
};

/** Upload control for an `image` or `file` field: choose or drop, progress, errors, remove. */
export function BlobInput(p: BlobInputProps) {
  const { field: f, value, onChange, readOnly } = p;
  const d = useData();
  const isImage = f.kind === "image";
  const max = f.max_bytes ?? (isImage ? 5 * 1024 * 1024 : 25 * 1024 * 1024);
  const input = useRef<HTMLInputElement>(null);
  const job = useRef<Upload | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState("");
  const [over, setOver] = useState(false);
  const hintId = useId();

  useEffect(() => () => job.current?.abort(), []);

  async function send(file: File | undefined) {
    if (!file || readOnly) return;
    setError("");
    if (isImage && !IMAGE_TYPES.includes(file.type)) {
      return setError(t("Use a PNG, JPEG, WebP or GIF image. SVG files are not accepted."));
    }
    if (file.size > max) {
      return setError(t("This file is larger than {size}.", { size: formatBytes(max) }));
    }
    if (!f.model) return setError(t("This field cannot take uploads."));
    setProgress(0);
    const up = uploadBlob(d.token, `${f.model}.${f.name}`, file, setProgress);
    job.current = up;
    const r = await up.promise;
    job.current = null;
    setProgress(null);
    if ("ok" in r) {
      onChange(r.ok.id);
      if (input.current) input.current.value = "";
    } else if (r.status === -1) {
      // cancelled
    } else if (r.status === 413) {
      setError(t("This file is larger than {size}.", { size: formatBytes(max) }));
    } else if (r.status === 429) {
      setError(t("Too many uploads. Please wait a few minutes."));
    } else if (r.status === 0) {
      setError(t("The upload did not go through. Check your connection and try again."));
    } else {
      setError(r.error || t("The upload was refused."));
    }
  }

  const busy = progress !== null;
  const pct = Math.round((progress ?? 0) * 100);
  const title = fieldTitle(f);
  return (
    <div
      className="blobfield"
      data-over={over || undefined}
      onDragOver={(e) => {
        if (readOnly || busy) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        void send(e.dataTransfer.files[0]);
      }}
    >
      <div className="blobrow">
        {isImage ? (
          value ? (
            <BlobThumb id={value} alt={title} size={96} className="preview" />
          ) : (
            <span className="blobthumb preview empty" style={{ inlineSize: 96, blockSize: 96 }}>
              <span className="blobthumb-ph">
                <Icon name="upload" size={22} />
              </span>
            </span>
          )
        ) : value ? (
          <FileChip id={value} field={f} />
        ) : (
          <span className="dim">{t("No file")}</span>
        )}
        {!readOnly && (
          <div className="blobactions">
            <button
              type="button"
              className="btn small"
              id={p.id}
              disabled={busy}
              aria-describedby={[hintId, p.describedBy].filter(Boolean).join(" ")}
              aria-invalid={p.invalid || undefined}
              onClick={() => input.current?.click()}
            >
              <Icon name="upload" size={14} />{" "}
              {isImage
                ? value
                  ? t("Replace image…")
                  : t("Choose image…")
                : value
                  ? t("Replace file…")
                  : t("Choose file…")}
            </button>
            {value && !busy && (
              <button type="button" className="btn small" onClick={() => onChange("")}>
                {t("Remove")}
              </button>
            )}
            {busy && (
              <button type="button" className="btn small" onClick={() => job.current?.abort()}>
                {t("Cancel")}
              </button>
            )}
          </div>
        )}
      </div>
      <input
        ref={input}
        type="file"
        hidden
        tabIndex={-1}
        accept={isImage ? IMAGE_ACCEPT : FILE_ACCEPT}
        onChange={(e) => void send(e.target.files?.[0])}
      />
      {busy && (
        <div
          className="blobprogress"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct}
          aria-label={t("Uploading… {percent}%", { percent: pct })}
        >
          <span style={{ inlineSize: `${pct}%` }} />
        </div>
      )}
      {!readOnly && (
        <small className="hint" id={hintId}>
          {isImage
            ? t("Drop an image here or choose one. PNG, JPEG, WebP or GIF, up to {size}.", {
                size: formatBytes(max),
              })
            : t("Drop a file here or choose one. Up to {size}.", { size: formatBytes(max) })}
          {isImage && p.altHint?.empty && value
            ? " " +
              t("Describe the picture in “{field}” for people who cannot see it.", {
                field: p.altHint.title,
              })
            : ""}
        </small>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </div>
  );
}
