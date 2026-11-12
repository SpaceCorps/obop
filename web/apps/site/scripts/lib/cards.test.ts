import { describe, expect, test } from "vite-plus/test";
import { messages } from "../../i18n/index.ts";
import { appView } from "./app-view.ts";
import { type App, loadApps } from "./apps.ts";
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
    // A card is given `keep` by hand, where its own words are a caveat that must not be missed (what an
    // AI agent may do without approval; a step that does not do what its name suggests: "By itself,
    // this does not notify anyone"), or speak of approval without the approval mark: a new one is a
    // deliberate change of this list. The
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
        "appointments: appointments_appointment.confirm",
        "connectors: connection.test",
        "connectors: data_flow_log",
        "email_marketing: email_marketing_campaign",
        "email_marketing: email_marketing_campaign.schedule",
        "expenses: expenses_expense.approve",
        "expenses: expenses_expense.submit",
        "fleet: fleet_vehicle_driver.suspend",
        "hr: hr_leave_request.request",
        "planning: planning_shift.publish",
        "purchase: purchase_order.send",
        "subscriptions: subscriptions_subscription.activate",
        "timesheets: timesheets_entry.approve",
        "timesheets: timesheets_entry.submit",
        "website: website_blog_post_translation",
        "website: website_menu_item",
        "website: website_page",
        "website: website_page_translation",
      ].sort(),
    );
    expect(chosen, "cards that have keep").toHaveLength(18);
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

  test("the cards whose words speak of approval, proposing or an AI agent, or say what they do not do, are never folded", () => {
    // What proceeds without approval, what needs a person's approval, what an AI agent may do or only
    // propose, and what a step does not do ("By itself, this does not notify anyone"): the statements
    // an owner must not miss. Matched on the English title and text of the card: every card that
    // speaks of approval, of proposing or of an AI agent, or says what it does not do, carries the
    // approval mark or `keep`, and is then checked in every language, since both are set by key.
    const SPEAKS = /approv|propos|\bAI\b/i;
    const NOT_DONE = /\bBy itself, (?:this|obop) does not\b/i;
    expect(SPEAKS.test("Edits to a published page need no further approval.")).toBe(true);
    expect(SPEAKS.test("an AI agent included, can do that without approval")).toBe(true);
    expect(SPEAKS.test("Submit the claim for approval")).toBe(true);
    expect(SPEAKS.test("Mark the entry as submitted, for a manager to approve or reject.")).toBe(
      true,
    );
    expect(SPEAKS.test("An AI agent can only propose it.")).toBe(true);
    expect(SPEAKS.test("Propose a change for a person to confirm")).toBe(true);
    expect(SPEAKS.test("Create a lead")).toBe(false);
    expect(NOT_DONE.test("Mark the campaign as scheduled. By itself, this does not send it.")).toBe(
      true,
    );
    expect(NOT_DONE.test("By itself, obop does not send it.")).toBe(true);
    expect(NOT_DONE.test("Create a lead")).toBe(false);
    const speaks: string[] = [];
    let notDone = 0;
    for (const a of apps)
      for (const section of SECTIONS) {
        const english = view(a.id)[section];
        english.forEach((c, i) => {
          if (NOT_DONE.test(c.text)) {
            notDone++;
            expect(
              isMarked(c),
              `${a.id}: "${c.title}" says what it does not do, and has neither the approval mark nor keep`,
            ).toBe(true);
          }
          if (!SPEAKS.test(`${c.title} ${c.text}`)) return;
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
    expect(speaks, "cards that speak of approval, proposing or an AI agent").toHaveLength(19);
    expect(notDone, "cards that say what they do not do").toBe(14);
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
    // Fleet: Retire and Suspend are marked (the approval mark, and "By itself, this does not stop the
    // driver being assigned a vehicle"), and the first three others are open beside them.
    expect(at("fleet", "actions")).toEqual([5, 2]);
    expect(titles(splitCards(view("fleet").actions).shown)).toEqual([
      "Dispatch",
      "Return to pool",
      "Retire",
      "Suspend",
      "Reactivate",
    ]);
    // Email Marketing: three marked cards (Schedule, Send, Send now) would show one other, but a fold
    // of one card is not made, so all five are open, as they were with two marked.
    expect(at("email_marketing", "actions")).toEqual([5, 0]);
    // The other pages whose cards say what they do not do keep all their cards open.
    for (const [id, section, n] of [
      ["purchase", "actions", 4],
      ["planning", "actions", 3],
      ["appointments", "actions", 4],
      ["email_marketing", "models", 3],
    ] as const)
      expect(at(id, section), `${id} ${section}`).toEqual([n, 0]);
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

describe("the app's own list of keys to keep (`AppCopy.keep`), read in every branch of the card view", () => {
  // Expenses has a model, "expenses_expense", and four actions: approve and pay have words of their
  // own in data/apps.ts, submit and refuse have not. A synthetic copy of it gets the cards and the
  // list each test needs, so each of the four places that read the list is run on its own.
  const expenses = app("expenses");
  const synthetic = (cards: App["cards"], keep: string[]): App => ({ ...expenses, cards, keep });
  const actions = (a: App, locale: Locale = "en") =>
    appView(a, messages[locale], apps, locale).actions;
  const models = (a: App, locale: Locale = "en") =>
    appView(a, messages[locale], apps, locale).models;
  const byTitle = <T extends { title: string }>(cards: T[], title: string): T =>
    cards.find((c) => c.title === title)!;
  const own = { text: "Words of its own, written for the card." };

  /** Every key a card of the app has: `model.action` for an action card, the model's name for a model card. */
  const cardKeys = (a: App) =>
    new Set([
      ...a.models.flatMap((m) => m.actions.map((x) => `${m.name}.${x.name}`)),
      ...a.models.map((m) => m.name),
    ]);
  /** The keys that data/apps.ts marks `keep` (on a card's own words, or in the app's list) and that no card of the app has. */
  const keepWithoutCard = (a: App) =>
    [
      ...Object.entries(a.cards)
        .filter(([, c]) => c.keep)
        .map(([key]) => key),
      ...a.keep,
    ].filter((key) => !cardKeys(a).has(key));

  test("an action card with words of its own is kept by the list alone", () => {
    const withList = synthetic({ "expenses_expense.pay": own }, ["expenses_expense.pay"]);
    expect(byTitle(actions(withList), "Pay").keep).toBe(true);
    const without = synthetic({ "expenses_expense.pay": own }, []);
    expect(byTitle(actions(without), "Pay").keep).toBeUndefined();
    expect(byTitle(actions(without), "Pay").text).toBe(own.text);
  });

  test("an action card without words of its own is kept by the list, and keeps its translation", () => {
    for (const locale of locales) {
      // The fourth action of Expenses is "Refuse" (no words of its own, and no approval mark here).
      const withList = actions(synthetic({}, ["expenses_expense.refuse"]), locale);
      const without = actions(synthetic({}, []), locale);
      expect(withList[3]!.keep, `${locale}: the fourth action is kept`).toBe(true);
      expect(without[3]!.keep, `${locale}: and is not without the list`).toBeUndefined();
      expect(withList[3]!.text, `${locale}: it keeps the product's words`).toBe(without[3]!.text);
      expect(withList[3]!.lang, locale).toBe(without[3]!.lang);
      // Only the action named in the list is kept.
      expect(withList.filter((c) => c.keep)).toHaveLength(1);
    }
  });

  test("a model card with words of its own is kept by the list alone", () => {
    const withList = synthetic({ expenses_expense: own }, ["expenses_expense"]);
    expect(models(withList)[0]!.keep).toBe(true);
    const without = synthetic({ expenses_expense: own }, []);
    expect(models(without)[0]!.keep).toBeUndefined();
    expect(models(without)[0]!.text).toBe(own.text);
  });

  test("a model card without words of its own is kept by the list, and keeps its translation", () => {
    for (const locale of locales) {
      const withList = models(synthetic({}, ["expenses_expense"]), locale);
      const without = models(synthetic({}, []), locale);
      expect(withList[0]!.keep, `${locale}: the model card is kept`).toBe(true);
      expect(without[0]!.keep, `${locale}: and is not without the list`).toBeUndefined();
      expect(withList[0]!.text, `${locale}: it keeps the product's words`).toBe(without[0]!.text);
      expect(withList[0]!.lang, locale).toBe(without[0]!.lang);
    }
  });

  test("a key that matches no card is found, with the key in the message", () => {
    expect(keepWithoutCard(synthetic({}, ["expenses_expense.sbumit"]))).toEqual([
      "expenses_expense.sbumit",
    ]);
    expect(keepWithoutCard(synthetic({ expenses_expens: { ...own, keep: true } }, []))).toEqual([
      "expenses_expens",
    ]);
    expect(keepWithoutCard(synthetic({}, ["expenses_expense.submit", "expenses_expense"]))).toEqual(
      [],
    );
    // In the real data every key of `keep`, in a card's own words or in an app's list, is a card.
    for (const a of apps)
      for (const key of [
        ...Object.entries(a.cards)
          .filter(([, c]) => c.keep)
          .map(([k]) => k),
        ...a.keep,
      ])
        expect(
          cardKeys(a).has(key),
          `${a.id}: keep names "${key}", which is no card of the app`,
        ).toBe(true);
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
