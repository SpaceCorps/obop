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

function Held() {
  const [q, set] = useState<LeaveQuestion | null>(null);
  setHold = set;
  useLeaveHold(q);
  return null;
}
function Page() {
  const [route, g] = useRoute();
  go = g;
  return (
    <>
      <Held />
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

  it("asks the unsaved-changes question when the hold has no wording for both", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await act(async () => setHold(hold));
    await leaveTo();
    expect(ask).toHaveBeenCalledWith(UNSAVED, "Leave and discard", "Keep editing");
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

  it("asks nothing when nothing is unsaved and nothing is held", async () => {
    const ask = answerWith(false);
    await change();
    expect(ask).not.toHaveBeenCalled();
    expect(text()).toBe("elsewhere");
  });
});
