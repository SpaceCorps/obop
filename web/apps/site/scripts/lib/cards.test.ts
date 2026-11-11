import { describe, expect, test } from "vite-plus/test";
import { messages } from "../../i18n/index.ts";
import { appView } from "./app-view.ts";
import { loadApps } from "./apps.ts";
import { SHOWN_CARDS, drawsKicker, splitCards } from "./cards.ts";
import { technicalIn } from "./plain.ts";

const apps = loadApps();
const app = (id: string) => apps.find((a) => a.id === id)!;
const view = (id: string, locale: keyof typeof messages = "en") =>
  appView(app(id), messages[locale], apps, locale);
const card = (title: string, approval = false) => ({ title, approval });
const titles = (cards: { title: string }[]) => cards.map((c) => c.title);

describe("which cards a section shows and which it folds", () => {
  test("three are shown; a fold is only made for two cards or more", () => {
    expect(SHOWN_CARDS).toBe(3);
    const plain = (n: number) => Array.from({ length: n }, (_, i) => card(String(i + 1)));
    for (const n of [0, 1, 2, 3, 4]) {
      const { shown, folded } = splitCards(plain(n));
      expect(shown, `${n} cards`).toHaveLength(n);
      expect(folded, `${n} cards`).toHaveLength(0);
    }
    expect(titles(splitCards(plain(5)).shown)).toEqual(["1", "2", "3"]);
    expect(titles(splitCards(plain(5)).folded)).toEqual(["4", "5"]);
    expect(titles(splitCards(plain(8)).folded)).toEqual(["4", "5", "6", "7", "8"]);
  });

  test("a card with the approval mark is never folded, however late it comes and however many there are", () => {
    // The marked ones first claim the room, the first of the others fill what is left of three.
    const late = [card("a"), card("b"), card("c"), card("d"), card("E", true), card("F", true)];
    expect(titles(splitCards(late).shown)).toEqual(["a", "E", "F"]);
    expect(titles(splitCards(late).folded)).toEqual(["b", "c", "d"]);
    // More marked cards than three: all of them are shown and nothing else.
    const many = [
      card("A", true),
      card("b"),
      card("C", true),
      card("D", true),
      card("e"),
      card("F", true),
    ];
    expect(titles(splitCards(many).shown)).toEqual(["A", "C", "D", "F"]);
    expect(titles(splitCards(many).folded)).toEqual(["b", "e"]);
    // The order is the one the cards came in, shown and folded alike, and no card is lost or doubled.
    for (const cards of [late, many]) {
      const { shown, folded } = splitCards(cards);
      expect([...shown, ...folded].map((c) => c.title).sort()).toEqual(titles(cards).sort());
      for (const part of [shown, folded])
        expect(part.map((c) => cards.indexOf(c))).toEqual(
          part.map((c) => cards.indexOf(c)).sort((x, y) => x - y),
        );
    }
    // A fold of one card is not made: the one that would be left alone is shown too.
    const one = [card("A", true), card("B", true), card("C", true), card("D", true), card("e")];
    expect(splitCards(one).folded).toHaveLength(0);
    expect(splitCards(one).shown).toHaveLength(5);
  });

  test("in the real data no card with the approval mark is in a fold, in any app", () => {
    let folds = 0;
    let marked = 0;
    for (const a of apps) {
      const v = appView(a, messages.en, apps, "en");
      const { shown, folded } = splitCards(v.actions);
      expect(
        folded.filter((c) => c.approval).map((c) => c.title),
        `${a.id}: marked cards in the fold`,
      ).toEqual([]);
      expect(shown.length + folded.length, a.id).toBe(v.actions.length);
      if (folded.length) folds++;
      marked += shown.filter((c) => c.approval).length;
    }
    // Not vacuous: there are folds, and marked cards to keep out of them.
    expect(folds).toBeGreaterThan(5);
    expect(marked).toBeGreaterThan(20);
    // The two pages where the first three in the manifest's order had no marked card among them.
    const subs = splitCards(view("subscriptions").actions);
    expect(subs.shown.map((c) => c.approval)).toEqual([true, true, true]);
    expect(subs.folded).toHaveLength(3);
    const shop = splitCards(view("ecommerce").actions);
    expect(shop.shown.filter((c) => c.approval)).toHaveLength(2);
    expect(shop.shown).toHaveLength(3);
    expect(shop.folded).toHaveLength(5);
  });

  test("every card that is shown without opening a fold is in plain words, as the page now shows them", () => {
    // plain.ts `visible` still describes the first six of a section; the page shows the cards that
    // `splitCards` picks, which can include a marked card from further down. This holds the rule to
    // what is drawn.
    let checked = 0;
    for (const a of apps) {
      const v = appView(a, messages.en, apps, "en");
      for (const c of [...splitCards(v.actions).shown, ...splitCards(v.models).shown]) {
        expect(
          technicalIn(`${c.kicker ?? ""} ${c.title} ${c.text}`),
          `${a.id} card "${c.title}": ${c.text}`,
        ).toBeNull();
        checked++;
      }
    }
    expect(checked).toBeGreaterThan(100);
    // The rule can fail: a card with such a word is found.
    expect(technicalIn("Discount use A hash of the code")).not.toBeNull();
  });
});

describe("the small word above a card's title is drawn on every card or on none", () => {
  test("none when every card works on the same thing, every one when they work on several", () => {
    expect(drawsKicker(view("crm").actions), "CRM: four cards, one thing").toBe(false);
    expect(drawsKicker(view("accounting").actions), "Accounting: several things").toBe(true);
    expect(drawsKicker(view("projects").actions), "Project: three things").toBe(true);
    expect(drawsKicker([]), "no cards").toBe(false);
    // The cards of "What it keeps" have no word above the title at all.
    expect(drawsKicker(view("accounting").models)).toBe(false);
  });

  test("the things are told apart by the model, not by the word a translated page gives it", () => {
    // Expenses in Swedish: one model, two words for it (the product's "Utlägg" and the English
    // "Expense" on the card that is shown in English). One thing, so no word is drawn.
    const cards = view("expenses", "sv").actions;
    expect(new Set(cards.map((c) => c.model)).size).toBe(1);
    expect(new Set(cards.map((c) => c.kicker)).size).toBeGreaterThan(1);
    expect(drawsKicker(cards)).toBe(false);
    // By the word alone it would have been drawn, as a change of thing that is none.
    expect(drawsKicker(cards.map((c) => ({ kicker: c.kicker })))).toBe(true);
  });
});
