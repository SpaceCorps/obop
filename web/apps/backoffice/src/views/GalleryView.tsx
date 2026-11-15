import { useEffect, useMemo, useState } from "react";
import { fmt, t } from "@obop/ui";
import { search, type FieldMeta, type Row } from "../api.ts";
import { useData } from "../data.tsx";
import { defaultOrder } from "../presentation.ts";
import { serverMessage } from "../serverMessage.ts";
import { EmptyState } from "../ui.tsx";
import { BoardCard, cardTitle, usePresentCtx } from "./BoardCard.tsx";
import { boardLayout, coverFieldOf } from "./board.ts";
import type { ViewProps } from "./types.ts";

/** Cards the gallery asks for at first, and each time "Load more" is used. */
export const GALLERY_PAGE = 48;

/** Whether a model can be shown as a gallery: it has a picture the person may see. */
export const hasGallery = (fields: FieldMeta[]): boolean => !!coverFieldOf(fields);

/**
 * The records as picture cards, without grouping (products, contacts, listings, employees): the
 * first image field is the cover, the rest of the card is the card layout. With a server search
 * (D4) it asks for one page at a time, in the search's order or the model's own (newest first);
 * otherwise it shows the records it is given.
 */
export function GalleryView(p: ViewProps) {
  const d = useData();
  const { model, fields } = p;
  const ctx = usePresentCtx(d.models.get(model.name)?.module.id);
  const cover = coverFieldOf(fields);
  const layout = useMemo(() => {
    const l = boardLayout(model, fields);
    // The cover is the picture: no avatar beside the title as well.
    delete l.avatar;
    return l;
  }, [model, fields]);
  const [limit, setLimit] = useState(GALLERY_PAGE);
  const [attempt, setAttempt] = useState(0);
  const [st, setSt] = useState<{ rows: Row[]; total: number; loading: boolean; error?: string }>(
    () => ({ rows: [], total: 0, loading: !!p.server }),
  );
  const server = p.server;
  const order = server?.order.length ? server.order : defaultOrder(model);
  const filterSig = JSON.stringify(server?.filter ?? null);
  const key = JSON.stringify([model.name, filterSig, order, limit, d.mutations, attempt]);
  useEffect(() => setLimit(GALLERY_PAGE), [model.name, filterSig]);
  useEffect(() => {
    if (!server) return;
    let live = true;
    setSt((s) => ({ ...s, loading: true }));
    void search(d.token, model.name, { filter: server.filter, order, limit }).then((r) => {
      if (!live) return;
      if ("error" in r) setSt((s) => ({ ...s, loading: false, error: r.error }));
      else setSt({ rows: r.rows, total: r.total, loading: false });
    });
    return () => {
      live = false;
    };
    // key stands for the model, the search, the order, the page, the writes and a retry
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, d.token]);

  const rows = server ? st.rows : p.rows;
  const total = server ? st.total : p.rows.length;
  if (server && st.error)
    return (
      <EmptyState
        title={t("Could not load records")}
        hint={serverMessage(st.error)}
        action={{ label: t("Try again"), onClick: () => setAttempt((n) => n + 1) }}
      />
    );

  return (
    <>
      <div
        className="gallery"
        role="list"
        aria-label={p.title}
        aria-busy={server && st.loading ? true : undefined}
      >
        {rows.map((r) => {
          const title = cardTitle(model, r, layout, ctx);
          return (
            <div role="listitem" key={r.id} className="gallery-item">
              <BoardCard
                model={model}
                fields={fields}
                row={r}
                layout={layout}
                ctx={ctx}
                title={title}
                cover={cover}
                selected={r.id === p.selectedId}
                draggable={false}
                dragging={false}
                menu={[]}
                menuLabel=""
                onOpen={() => p.onOpen(r.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    p.onOpen(r.id);
                  }
                }}
              />
            </div>
          );
        })}
      </div>
      {server && total > rows.length && (
        <div className="gallery-more">
          <span>
            {t("Showing {shown} of {total}", {
              shown: fmt.integer(rows.length),
              total: fmt.integer(total),
            })}
          </span>
          <button
            className="btn small"
            disabled={st.loading}
            onClick={() => setLimit((n) => n + GALLERY_PAGE)}
          >
            {st.loading
              ? t("Loading…")
              : t("Load more ({count} left)", { count: total - rows.length })}
          </button>
        </div>
      )}
    </>
  );
}
