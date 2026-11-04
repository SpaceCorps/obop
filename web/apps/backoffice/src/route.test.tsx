// @vitest-environment jsdom
// What the route asks before it lets go of the page (route.ts, leaveGuard.ts): unsaved changes in
// the form (`nav.dirty`, set by the record panel), something that is lost for good (a hold,
// `useLeaveHold`: the signing link that is shown once), both, or neither. Tested through the two ways
// out the app uses: `go()` (the palette, the panel's close button) and the browser's own address
// change (Back, a swipe, a typed address). The question is `nav.ask`, which App.tsx wires to its
// confirm dialog; here it is a spy.
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { useLeaveHold, type LeaveQuestion } from "./leaveGuard.ts";
import { nav, useRoute, type Go } from "./route.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let go: Go;
let setHold: (q: LeaveQuestion | null) => void;
let setHold2: (q: LeaveQuestion | null) => void;

function Held() {
  const [q, set] = useState<LeaveQuestion | null>(null);
  setHold = set;
  useLeaveHold(q);
  return null;
}
/** A second thing that can be lost, held at the same time. */
function HeldToo() {
  const [q, set] = useState<LeaveQuestion | null>(null);
  setHold2 = set;
  useLeaveHold(q);
  return null;
}
function Page() {
  const [route, g] = useRoute();
  go = g;
  return (
    <>
      <Held />
      <HeldToo />
      <p>{route.model === "lead" ? "elsewhere" : "here"}</p>
    </>
  );
}

const UNSAVED = "You have unsaved changes. Leave and discard them?";
const hold: LeaveQuestion = {
  message: "The thing is lost if you leave. Leave anyway?",
  leave: "Leave without it",
  stay: "Stay with it",
};
const both = "You have unsaved changes, and the thing is lost. Leave anyway?";
const second: LeaveQuestion = {
  message: "The other thing is lost as well. Leave anyway?",
  leave: "Leave without the other",
  stay: "Stay with the other",
};

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
const text = () => container.textContent ?? "";
const answerWith = (leave: boolean) => {
  const ask = vi.fn(async (_message: string, _leave: string, _stay: string) => leave);
  nav.ask = ask;
  return ask;
};
/** Answer the questions one after the other: the first call gets the first answer, and so on. */
const answerInTurn = (...leave: boolean[]) => {
  let n = 0;
  const ask = vi.fn(async (_message: string, _leave: string, _stay: string) => leave[n++] ?? false);
  nav.ask = ask;
  return ask;
};

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  history.replaceState(null, "", "/");
  nav.ask = null;
  nav.dirty = null;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => {
    root.render(<Page />);
  });
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  nav.ask = null;
  nav.dirty = null;
});

describe("go(): the palette, the panel's close button", () => {
  const leaveTo = async () => {
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
  };

  it("asks nothing when nothing is unsaved and nothing is held", async () => {
    const ask = answerWith(false);
    await leaveTo();
    expect(ask).not.toHaveBeenCalled();
    expect(text()).toBe("elsewhere");
  });

  it("asks the unsaved-changes question, with its two buttons, when the form is dirty", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await leaveTo();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask).toHaveBeenCalledWith(UNSAVED, "Leave and discard", "Keep editing");
    expect(text(), "went although they chose to keep editing").toBe("here");
  });

  it("goes when the person answers that they leave and discard", async () => {
    answerWith(true);
    nav.dirty = () => true;
    await leaveTo();
    expect(text()).toBe("elsewhere");
  });

  it("asks the held question, with its buttons, when something is lost for good", async () => {
    const ask = answerWith(false);
    await act(async () => setHold(hold));
    await leaveTo();
    expect(ask).toHaveBeenCalledWith(hold.message, hold.leave, hold.stay);
    expect(text()).toBe("here");
  });

  it("asks one question that names both when the form is dirty and something is held", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await act(async () => setHold({ ...hold, whenUnsaved: both }));
    await leaveTo();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask).toHaveBeenCalledWith(both, "Leave anyway", "Stay");
    expect(text()).toBe("here");
  });

  it("asks the held question after the unsaved-changes question when the hold has no wording for both", async () => {
    const ask = answerInTurn(true, false);
    nav.dirty = () => true;
    await act(async () => setHold(hold));
    await leaveTo();
    expect(ask.mock.calls).toEqual([
      [UNSAVED, "Leave and discard", "Keep editing"],
      [hold.message, hold.leave, hold.stay],
    ]);
    expect(text(), "went although the second question was answered with stay").toBe("here");
  });

  it("goes only when every question is answered with leave", async () => {
    const ask = answerInTurn(true, true);
    nav.dirty = () => true;
    await act(async () => setHold(hold));
    await leaveTo();
    expect(ask).toHaveBeenCalledTimes(2);
    expect(text()).toBe("elsewhere");
  });

  it("asks no further question once one is answered with stay", async () => {
    const ask = answerInTurn(false, true);
    nav.dirty = () => true;
    await act(async () => setHold(hold));
    await leaveTo();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask.mock.calls[0]![0]).toBe(UNSAVED);
    expect(text()).toBe("here");
  });

  it("asks about every thing that is held, in the order they were taken", async () => {
    const ask = answerInTurn(true, true);
    await act(async () => setHold(hold));
    await act(async () => setHold2(second));
    await leaveTo();
    expect(ask.mock.calls).toEqual([
      [hold.message, hold.leave, hold.stay],
      [second.message, second.leave, second.stay],
    ]);
    expect(text()).toBe("elsewhere");
  });

  it("stays when the second of two held things is answered with stay", async () => {
    const ask = answerInTurn(true, false);
    await act(async () => setHold(hold));
    await act(async () => setHold2(second));
    await leaveTo();
    expect(ask).toHaveBeenCalledTimes(2);
    expect(text()).toBe("here");
  });

  it("joins the unsaved changes to the first hold and asks the other hold on its own", async () => {
    const ask = answerInTurn(true, true);
    nav.dirty = () => true;
    await act(async () => setHold({ ...hold, whenUnsaved: both }));
    await act(async () => setHold2(second));
    await leaveTo();
    expect(ask.mock.calls).toEqual([
      [both, "Leave anyway", "Stay"],
      [second.message, second.leave, second.stay],
    ]);
  });

  it("asks about a hold that has no wording for the unsaved changes even when it is the second", async () => {
    const ask = answerInTurn(true, true, true);
    nav.dirty = () => true;
    await act(async () => setHold(hold));
    await act(async () => setHold2(second));
    await leaveTo();
    expect(ask.mock.calls.map((c) => c[0])).toEqual([UNSAVED, hold.message, second.message]);
  });

  it("asks no more about a hold that was given up", async () => {
    const ask = answerInTurn(true, true);
    await act(async () => setHold(hold));
    await act(async () => setHold2(second));
    await act(async () => setHold(null));
    await leaveTo();
    expect(ask.mock.calls.map((c) => c[0])).toEqual([second.message]);
  });

  it("stops asking about the held thing once it is given up, and still asks about the form", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await act(async () => setHold({ ...hold, whenUnsaved: both }));
    await act(async () => setHold(null));
    await leaveTo();
    expect(ask).toHaveBeenCalledWith(UNSAVED, "Leave and discard", "Keep editing");
  });

  it("goes without asking when nothing can ask", async () => {
    nav.dirty = () => true;
    await act(async () => setHold(hold));
    await leaveTo();
    expect(text()).toBe("elsewhere");
  });
});

describe("go() to the address that is already shown (Save, Ctrl+S under the open dialog)", () => {
  // Saving a record goes to the record's own address, which is the one on screen. Nothing is left, so
  // nothing is asked, whatever is unsaved or held: a question about a link that is not going
  // anywhere teaches the person to click "Leave anyway" on the one prompt that matters.
  const lead = async () => {
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
  };

  it("asks nothing when the address does not change, although the form is dirty and a hold exists", async () => {
    await lead();
    const ask = answerWith(false);
    nav.dirty = () => true;
    await act(async () => setHold({ ...hold, whenUnsaved: both }));
    await lead();
    expect(ask).not.toHaveBeenCalled();
    expect(text()).toBe("elsewhere");
  });

  it("counts the home address as unchanged when the page opened with no address at all", async () => {
    const ask = answerWith(false);
    await act(async () => setHold(hold));
    await act(async () => go({}));
    await settle();
    expect(ask).not.toHaveBeenCalled();
  });

  it("counts an empty view as no view", async () => {
    await lead();
    const ask = answerWith(false);
    await act(async () => setHold(hold));
    await act(async () => go({ app: "crm", model: "lead", view: "" }));
    await settle();
    expect(ask).not.toHaveBeenCalled();
  });

  it("still asks when the record or the view in the address changes", async () => {
    await lead();
    const ask = answerWith(false);
    await act(async () => setHold(hold));
    await act(async () => go({ app: "crm", model: "lead", id: "other" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    await act(async () => go({ app: "crm", model: "lead", view: "kanban" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(2);
    await act(async () => go({ app: "sales", model: "lead" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(3);
  });
});

describe("the browser's own address change: Back, a swipe", () => {
  const change = async () => {
    await act(async () => {
      location.hash = "#/crm/lead";
    });
    await settle();
  };

  it("asks the unsaved-changes question and puts the address back", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    const here = location.hash;
    await change();
    expect(ask).toHaveBeenCalledWith(UNSAVED, "Leave and discard", "Keep editing");
    expect(location.hash).toBe(here || "#/");
    expect(text()).toBe("here");
  });

  it("goes to the new address when the person leaves and discards", async () => {
    answerWith(true);
    nav.dirty = () => true;
    await change();
    await settle();
    expect(location.hash).toBe("#/crm/lead");
    expect(text()).toBe("elsewhere");
  });

  it("asks the one question that names both", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await act(async () => setHold({ ...hold, whenUnsaved: both }));
    await change();
    expect(ask).toHaveBeenCalledWith(both, "Leave anyway", "Stay");
  });

  it("asks about every thing that is held, and goes only when each is answered with leave", async () => {
    const ask = answerInTurn(true, true);
    await act(async () => setHold(hold));
    await act(async () => setHold2(second));
    await change();
    await settle();
    expect(ask.mock.calls.map((c) => c[0])).toEqual([hold.message, second.message]);
    expect(location.hash).toBe("#/crm/lead");
    expect(text()).toBe("elsewhere");
  });

  it("puts the address back and stays when the second question is answered with stay", async () => {
    const ask = answerInTurn(true, false);
    await act(async () => setHold(hold));
    await act(async () => setHold2(second));
    const here = location.hash;
    await change();
    await settle();
    expect(ask).toHaveBeenCalledTimes(2);
    expect(location.hash).toBe(here || "#/");
    expect(text()).toBe("here");
  });

  it("asks nothing when nothing is unsaved and nothing is held", async () => {
    const ask = answerWith(false);
    await change();
    expect(ask).not.toHaveBeenCalled();
    expect(text()).toBe("elsewhere");
  });
});
