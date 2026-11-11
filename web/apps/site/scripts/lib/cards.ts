/**
 * Which cards of a section an app page shows and which it folds, and whether the small word above the
 * titles is drawn. Pure functions, shared by the page (`AppCards.vue`) and the unit tests, so the rule
 * that is tested is the rule that draws.
 */

/** How many cards a section shows before it folds the rest ("What you can do", "What it keeps"). */
export const SHOWN_CARDS = 3;

/**
 * A card that is never folded: it carries the approval mark (a step that needs a person's approval
 * when an AI agent asks) or `keep` (its own words are a caveat that must not be missed: what an AI
 * agent may do without approval, that a step starts no billing; set in `data/apps.ts`, by the card's
 * source key, so every language has it).
 */
export const isMarked = (c: { approval?: boolean; keep?: boolean }): boolean =>
  !!(c.approval || c.keep);

/**
 * The cards that are shown and the cards that go in the fold, both in the order they came in (the
 * manifest's). Every marked card (`isMarked`) is shown, however many there are and however late they
 * come. Marked cards never take all the room: the first of the others fill what is left of `limit`,
 * and at least one of them is shown, so a section is never only its exceptions (a page whose "What you
 * can do" is three cancellations, its first step folded). A fold is only made for two cards or more
 * ("Show 1 more actions" is not a sentence in most languages): where one card would be left in it, it
 * is shown too, and so are all of them when there are at most `limit` + 1.
 */
export function splitCards<T extends { approval?: boolean; keep?: boolean }>(
  cards: readonly T[],
  limit: number = SHOWN_CARDS,
): { shown: T[]; folded: T[] } {
  const marked = cards.filter(isMarked).length;
  let room = Math.max(1, limit - marked);
  const shown: T[] = [];
  const folded: T[] = [];
  for (const c of cards) {
    if (isMarked(c)) shown.push(c);
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
