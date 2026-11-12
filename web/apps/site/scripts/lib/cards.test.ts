import { describe, expect, test } from "vite-plus/test";
import { messages } from "../../i18n/index.ts";
import { appView } from "./app-view.ts";
import { loadApps } from "./apps.ts";
import { SHOWN_CARDS, drawsKicker, isMarked, splitCards } from "./cards.ts";
import { technicalIn } from "./plain.ts";

const apps = loadApps();
const app = (id: string) => apps.find((a) => a.id === id)!;
type Locale = keyof typeof messages;
const locales = Object.keys(messages) as Locale[];
const view = (id: string, locale: Locale = "en") =>
  appView(app(id), messages[locale], apps, locale);
const SECTIONS = ["actions", "models"] as const;
const card = (title: string, approval = false) => ({ title, approval });
const kept = (title: string) => ({ title, keep: true });
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

  test("a marked card is never folded, however late it comes and however many there are", () => {
    // The marked ones are shown wherever they stand, and with fewer than three of them the first three
    // of the others are shown beside them.
    const late = [
      card("a"),
      card("b"),
      card("c"),
      card("d"),
      card("e"),
      card("F", true),
      card("G", true),
    ];
    expect(titles(splitCards(late).shown)).toEqual(["a", "b", "c", "F", "G"]);
    expect(titles(splitCards(late).folded)).toEqual(["d", "e"]);
    // More marked cards than three: all of them are shown, and the first other card with them.
    const many = [
      card("A", true),
      card("b"),
      card("C", true),
      card("D", true),
      card("e"),
      card("f"),
      card("G", true),
    ];
    expect(titles(splitCards(many).shown)).toEqual(["A", "b", "C", "D", "G"]);
    expect(titles(splitCards(many).folded)).toEqual(["e", "f"]);
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
    const alone = [card("A", true), card("b"), card("C", true), card("D", true), card("e")];
    expect(splitCards(alone).folded).toHaveLength(0);
    // The same with fewer than three marked: three others are shown, the fourth would be alone.
    const fourth = [card("a"), card("b"), card("c"), card("d"), card("E", true)];
    expect(splitCards(fourth).folded).toHaveLength(0);
    expect(splitCards(fourth).shown).toHaveLength(5);
  });

  test("the room beside the marked cards: three others beside fewer than three, one beside three or more", () => {
    // Three cancellations and then the steps that lead up to them: the page said "What you can do:
    // cancel", with its first step folded. With three or more marked cards one of the others is
    // shown, in the manifest's order, so a section is never only its exceptions.
    const cancels = [
      card("Activate"),
      card("Mark past due"),
      card("Recover"),
      card("Cancel trial", true),
      card("Cancel", true),
      card("Cancel overdue", true),
    ];
    expect(titles(splitCards(cancels).shown)).toEqual([
      "Activate",
      "Cancel trial",
      "Cancel",
      "Cancel overdue",
    ]);
    expect(titles(splitCards(cancels).folded)).toEqual(["Mark past due", "Recover"]);
    // Where fewer than three are marked, three of the others are shown beside them (not what is left
    // of three: the marked cards do not take the room of the others).
    const one = [card("a"), card("b"), card("c"), card("d"), card("E", true), card("f")];
    expect(titles(splitCards(one).shown)).toEqual(["a", "b", "c", "E"]);
    expect(titles(splitCards(one).folded)).toEqual(["d", "f"]);
    const two = [
      card("a"),
      card("B", true),
      card("c"),
      card("D", true),
      card("e"),
      card("f"),
      card("g"),
    ];
    expect(titles(splitCards(two).shown)).toEqual(["a", "B", "c", "D", "e"]);
    expect(titles(splitCards(two).folded)).toEqual(["f", "g"]);
    const none = [card("a"), card("b"), card("c"), card("d"), card("e")];
    expect(titles(splitCards(none).shown)).toEqual(["a", "b", "c"]);
    // At three marked cards the rule turns over: one of the others, not three.
    const three = [
      card("a"),
      card("B", true),
      card("c"),
      card("D", true),
      card("e"),
      card("F", true),
      card("g"),
    ];
    expect(titles(splitCards(three).shown)).toEqual(["a", "B", "D", "F"]);
    expect(titles(splitCards(three).folded)).toEqual(["c", "e", "g"]);
    // The limit is a parameter of the rule, not a number in it.
    expect(titles(splitCards(none, 2).shown)).toEqual(["a", "b"]);
    expect(titles(splitCards([card("a"), card("B", true), card("c"), card("d")], 1).shown)).toEqual(
      ["a", "B"],
    );
  });

  test("a card with `keep` is held like a card with the approval mark", () => {
    expect(isMarked(card("x", true))).toBe(true);
    expect(isMarked(kept("x"))).toBe(true);
    expect(isMarked(card("x"))).toBe(false);
    expect(isMarked({ approval: false, keep: false })).toBe(false);
    const cards = [card("a"), card("b"), card("c"), card("d"), kept("E"), card("f")];
    expect(titles(splitCards(cards).shown)).toEqual(["a", "b", "c", "E"]);
    expect(titles(splitCards(cards).folded)).toEqual(["d", "f"]);
    // It counts with the approval mark, and it is shown however late it comes and however many are.
    const mixed = [
      card("a"),
      card("b"),
      card("c"),
      kept("D"),
      card("E", true),
      kept("F"),
      card("g"),
      card("h"),
    ];
    expect(titles(splitCards(mixed).shown)).toEqual(["a", "D", "E", "F"]);
    expect(titles(splitCards(mixed).folded)).toEqual(["b", "c", "g", "h"]);
  });

  test("in the real data no marked card is in a fold, on any app page in any language", () => {
    let folds = 0;
    let markedShown = 0;
    for (const a of apps)
      for (const section of SECTIONS) {
        const english = view(a.id)[section];
        const shownEn = english.map((c) => splitCards(english).shown.includes(c));
        for (const locale of locales) {
          const cards = view(a.id, locale)[section];
          const { shown, folded } = splitCards(cards);
          const where = `${locale} ${a.id} ${section}`;
          expect(
            folded.filter(isMarked).map((c) => c.title),
            `${where}: marked cards in the fold`,
          ).toEqual([]);
          expect(shown.length + folded.length, where).toBe(cards.length);
          // The marks come with the source key, not with the words: the same cards are shown in
          // every language, which a translated text cannot change.
          expect(
            cards.map((c) => shown.includes(c)),
            `${where}: the same cards as in English`,
          ).toEqual(shownEn);
          if (locale === "en") {
            if (folded.length) folds++;
            markedShown += shown.filter(isMarked).length;
          }
        }
      }
    // Not vacuous: there are folds, and marked cards to keep out of them.
    expect(folds).toBeGreaterThan(8);
    expect(markedShown).toBeGreaterThan(30);
  });

  test("the cards with `keep` are the ones that were chosen, and each reaches a card", () => {
    // A card is given `keep` by hand, where its own words are a caveat that must not be missed, or
    // speak of approval without the approval mark: a new one is a deliberate change of this list. The
    // flag is a card's own (`cards`) or, for a card that keeps the manifest's translated words, the
    // app's list of keys (`keep`).
    const chosen = apps.flatMap((a) => [
      ...Object.entries(a.cards)
        .filter(([, c]) => c.keep)
        .map(([key]) => `${a.id}: ${key}`),
      ...a.keep.map((key) => `${a.id}: ${key}`),
    ]);
    expect(chosen.sort()).toEqual(
      [
        "connectors: connection.test",
        "connectors: data_flow_log",
        "expenses: expenses_expense.approve",
        "expenses: expenses_expense.submit",
        "hr: hr_leave_request.request",
        "subscriptions: subscriptions_subscription.activate",
        "timesheets: timesheets_entry.approve",
        "timesheets: timesheets_entry.submit",
        "website: website_blog_post_translation",
        "website: website_menu_item",
        "website: website_page",
        "website: website_page_translation",
      ].sort(),
    );
    // Each of them is a card of the page (a key that matches no card would hold nothing), in every
    // language: the flag comes with the source key and not with the words.
    for (const a of apps)
      for (const locale of locales) {
        const n = Object.values(a.cards).filter((c) => c.keep).length + a.keep.length;
        const onPage = SECTIONS.flatMap((section) => view(a.id, locale)[section]).filter(
          (c) => c.keep,
        );
        expect(onPage, `${locale} ${a.id}: the cards that have keep`).toHaveLength(n);
      }
  });

  test("a card that keeps the manifest's words keeps its translations", () => {
    // Expenses "Submit" has no words of its own in data/apps.ts (an entry there would make it English
    // on every page): `keep` of the app holds it by key and the product's translation stays.
    const en = view("expenses").actions.find((c) => c.title === "Submit")!;
    expect(en.keep).toBe(true);
    expect(en.lang).toBeUndefined();
    expect(en.text).toBe("Submit the claim for approval");
    const at = view("expenses").actions.findIndex((c) => c.title === "Submit");
    for (const locale of locales.filter((l) => l !== "en")) {
      const submit = view("expenses", locale).actions[at]!;
      expect(submit.keep, `${locale}: kept`).toBe(true);
      expect(submit.lang, `${locale}: the product's own words, not English`).toBeUndefined();
      expect(submit.text, locale).not.toBe(en.text);
    }
  });

  test("the cards whose words speak of an AI agent or of approval are never folded", () => {
    // What proceeds without approval, what needs a person's approval and what an AI agent may do: the
    // statements an owner must not miss. Matched on the English words of the card: every card whose
    // text speaks of approval or of an AI agent carries the approval mark or `keep`, and is then
    // checked in every language, since both are set by key.
    const SPEAKS = /approv|\bAI agents?\b/i;
    expect(SPEAKS.test("Edits to a published page need no further approval.")).toBe(true);
    expect(SPEAKS.test("an AI agent included, can do that without approval")).toBe(true);
    expect(SPEAKS.test("Submit the claim for approval")).toBe(true);
    expect(SPEAKS.test("Mark the entry as submitted, for a manager to approve or reject.")).toBe(
      true,
    );
    expect(SPEAKS.test("Create a lead")).toBe(false);
    const speaks: string[] = [];
    for (const a of apps)
      for (const section of SECTIONS) {
        const english = view(a.id)[section];
        english.forEach((c, i) => {
          if (!SPEAKS.test(c.text)) return;
          speaks.push(`${a.id}: ${c.title}`);
          expect(
            isMarked(c),
            `${a.id}: "${c.title}" speaks of approval or of an AI agent, and has neither the approval mark nor keep`,
          ).toBe(true);
          for (const locale of locales) {
            const cards = view(a.id, locale)[section];
            expect(
              splitCards(cards).shown,
              `${locale} ${a.id}: "${c.title}" speaks of approval and is folded`,
            ).toContain(cards[i]);
          }
        });
      }
    // Not vacuous: the four Website cards, the Contacts erase step and the two Submit cards that
    // have no approval mark are among them.
    for (const name of [
      "base: Erase",
      "expenses: Submit",
      "timesheets: Submit",
      "website: Blog post translation",
      "website: Menu item",
      "website: Page",
      "website: Page translation",
    ])
      expect(speaks, name).toContain(name);
    expect(speaks.length).toBeGreaterThan(8);
  });

  test("the cards a page opens with: the numbers on the pages that were named", () => {
    const at = (id: string, section: (typeof SECTIONS)[number]) => {
      const { shown, folded } = splitCards(view(id)[section]);
      return [shown.length, folded.length];
    };
    // Subscriptions: its lifecycle is in view with the cancellations, and nothing is folded.
    expect(at("subscriptions", "actions")).toEqual([6, 0]);
    expect(titles(splitCards(view("subscriptions").actions).shown)).toEqual([
      "Activate",
      "Mark past due",
      "Recover",
      "Cancel trial",
      "Cancel",
      "Cancel overdue",
    ]);
    // eCommerce: the two refunds and the first three other steps, in the manifest's order.
    expect(at("ecommerce", "actions")).toEqual([5, 3]);
    expect(titles(splitCards(view("ecommerce").actions).shown)).toEqual([
      "Mark paid",
      "Ship",
      "Deliver",
      "Refund",
      "Refund delivered",
    ]);
    expect(titles(splitCards(view("ecommerce").actions).folded)).toEqual([
      "Cancel",
      "Mark failed",
      "Cancel",
    ]);
    expect(splitCards(view("ecommerce").actions).shown.filter(isMarked)).toHaveLength(2);
    // Website: the four cards that state a caveat are shown, in the manifest's order; only the blog
    // post and the form submission are left, and a fold is not made for them. The page's Archive is
    // shown before the blog's Publish.
    expect(at("website", "models")).toEqual([6, 0]);
    expect(at("website", "actions")).toEqual([5, 3]);
    expect(titles(splitCards(view("website").actions).shown)).toEqual([
      "Publish",
      "Unpublish",
      "Archive",
      "Publish",
      "Unpublish",
    ]);
    expect(at("hr", "actions")).toEqual([5, 0]);
    // Nine marked cards and one other (three or more marked): the rule holds a long run to one other.
    expect(at("connectors", "actions")).toEqual([10, 3]);
    expect(at("accounting", "actions")).toEqual([7, 3]);
    expect(at("accounting", "models")).toEqual([3, 5]);
    // The two apps whose Submit and Approve cards speak of approval: nothing is folded, as before.
    expect(at("timesheets", "actions")).toEqual([3, 0]);
    expect(at("expenses", "actions")).toEqual([4, 0]);
    // The room: three others while fewer than three cards are marked, one other from three marked
    // cards on. No page opens with more cards than the marked ones and that room.
    for (const a of apps)
      for (const section of SECTIONS) {
        const cards = view(a.id)[section];
        const { shown, folded } = splitCards(cards);
        if (folded.length === 0) continue;
        const marked = cards.filter(isMarked).length;
        expect(shown, `${a.id} ${section}`).toHaveLength(
          marked + (marked >= SHOWN_CARDS ? 1 : SHOWN_CARDS),
        );
      }
    // The Get box follows these cards on a marketplace app: none of them opens with more than seven.
    const longest = Math.max(
      ...apps.filter((a) => !a.builtin).map((a) => splitCards(view(a.id).actions).shown.length),
    );
    expect(longest).toBe(7);
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
