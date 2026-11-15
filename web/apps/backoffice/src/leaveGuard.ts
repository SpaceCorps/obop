import { useEffect, useRef } from "react";

/**
 * Something on screen that is lost for good if the person leaves: a signing link that is shown only
 * once (D92) is the first. While one is held, every way out asks first: a change of the address (the
 * browser's Back button, a swipe, the command palette, closing the record panel: `route.ts`) and
 * leaving the page (reload, closing the tab: `beforeunload`). Nothing is stored; a hold lives only
 * as long as the component that made it.
 */
export type LeaveQuestion = {
  /** The question, in the person's language. */
  message: string;
  /** The button that goes ahead and loses it. */
  leave: string;
  /** The button that stays. */
  stay: string;
  /**
   * The question to ask instead when the person also has unsaved changes (`route.ts`), so that the
   * one question names both things they are about to lose. It is answered with "Leave anyway" and
   * "Stay". A hold without it is asked after the unsaved-changes question, as a question of its own.
   */
  whenUnsaved?: string;
};

const holds = new Set<() => LeaveQuestion | null>();

/** The questions to ask before leaving, one for every hold that is active, or none. */
export function heldLeaveQuestions(): LeaveQuestion[] {
  const out: LeaveQuestion[] = [];
  for (const ask of holds) {
    const q = ask();
    if (q) out.push(q);
  }
  return out;
}

/** The first question to ask before leaving, or `null` when nothing is held. */
export function heldLeaveQuestion(): LeaveQuestion | null {
  return heldLeaveQuestions()[0] ?? null;
}

/**
 * Hold `question` while it is not `null`; give it up when the component goes away. The browser's own
 * prompt for a reload or a closed tab comes with it (its words are the browser's, not ours).
 */
export function useLeaveHold(question: LeaveQuestion | null): void {
  const current = useRef(question);
  current.current = question;
  useEffect(() => {
    const ask = () => current.current;
    holds.add(ask);
    const before = (e: BeforeUnloadEvent) => {
      if (current.current) e.preventDefault();
    };
    addEventListener("beforeunload", before);
    return () => {
      holds.delete(ask);
      removeEventListener("beforeunload", before);
    };
  }, []);
}
