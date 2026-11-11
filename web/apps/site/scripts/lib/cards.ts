/**
 * Which cards of a section an app page shows and which it folds, and whether the small word above the
 * titles is drawn. Pure functions, shared by the page (`AppCards.vue`) and the unit tests, so the rule
 * that is tested is the rule that draws.
 */

/** How many cards a section shows before it folds the rest ("What you can do", "What it keeps"). */
export const SHOWN_CARDS = 3;

/**
 * The cards that are shown and the cards that go in the fold, both in the order they came in (the
 * manifest's). All are shown when there are at most `limit` + 1: a fold is only made for two cards or
 * more ("Show 1 more actions" is not a sentence in most languages). Otherwise every card that carries
 * the approval mark is shown, however many there are (a step that needs a person's approval is never
 * behind a tap), then the first of the others until `limit` are shown.
 */
export function splitCards<T extends { approval?: boolean }>(
  cards: readonly T[],
  limit: number = SHOWN_CARDS,
): { shown: T[]; folded: T[] } {
  if (cards.length <= limit + 1) return { shown: [...cards], folded: [] };
  const marked = cards.filter((c) => c.approval).length;
  let room = Math.max(0, limit - marked);
  const shown: T[] = [];
  const folded: T[] = [];
  for (const c of cards) {
    if (c.approval) shown.push(c);
    else if (room > 0) {
      shown.push(c);
      room--;
    } else folded.push(c);
  }
  return folded.length < 2 ? { shown: [...cards], folded: [] } : { shown, folded };
}

/**
 * Whether the small word above a card's title (the thing the card works on) is drawn. It says nothing
 * when every card of the section works on the same thing ("LEAD" four times): then it is drawn on
 * none. Where the cards work on several things it is drawn on every card, in the fold too, so no card
 * is left without its thing and a row of cards starts at one height. The things are told apart by the
 * model, not by the word: a translated page can name one model in two languages (the product's word on
 * one card, the English one on a card that is shown in English).
 */
export function drawsKicker(cards: readonly { model?: string; kicker?: string }[]): boolean {
  return new Set(cards.map((c) => c.model ?? c.kicker ?? "")).size > 1;
}
