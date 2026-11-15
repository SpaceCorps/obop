import { describe, expect, test } from "vite-plus/test";
import {
  centre,
  devtoolsSockets,
  hasText,
  homePackage,
  inputTextArgument,
  instancePage,
  nodeById,
  parseAccount,
  parseUiDump,
  resumedPackage,
} from "./adb-lib.mjs";

/** A cut of a real dump of the address screen and of the host bar. */
const DUMP = `<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><hierarchy rotation="0"><node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="org.obop.app.test" content-desc="" checkable="false" enabled="true" bounds="[0,0][1080,2400]"><node index="0" text="" resource-id="android:id/content" class="android.widget.FrameLayout" package="org.obop.app.test" content-desc="" enabled="true" bounds="[0,0][1080,2400]"><node index="0" text="http://localhost:18180" resource-id="address-field" class="android.widget.EditText" package="org.obop.app.test" content-desc="Server address" enabled="true" bounds="[42,456][1038,582]" /><node index="1" text="" resource-id="address-connect" class="android.view.ViewGroup" package="org.obop.app.test" content-desc="Connect" enabled="false" bounds="[42,2169][1038,2295]"><node index="0" text="Connect" resource-id="" class="android.widget.TextView" package="org.obop.app.test" content-desc="" enabled="true" bounds="[462,2204][619,2260]" /></node><node index="2" text="" resource-id="host-bar" class="android.view.ViewGroup" package="org.obop.app.test" content-desc="Server localhost:18180, not encrypted, test build" enabled="true" bounds="[0,136][1080,379]" /><node index="3" text="Tom &amp; Jerry&apos;s &lt;shop&gt; &#8250; &#x41;" resource-id="odd" class="android.widget.TextView" package="org.obop.app.test" content-desc="" enabled="true" bounds="[-5,0][10,20]" /><node index="4" text="no bounds" resource-id="lost" class="android.widget.TextView" /></node></node></hierarchy>`;

describe("the dump of the screen", () => {
  const nodes = parseUiDump(DUMP);

  test("every node with bounds, in document order", () => {
    expect(nodes.map((node) => node.id)).toEqual([
      "",
      "android:id/content",
      "address-field",
      "address-connect",
      "",
      "host-bar",
      "odd",
    ]);
  });

  test("a testID is the resource-id; the text, the description and the state come with it", () => {
    expect(nodeById(nodes, "address-field")).toEqual({
      id: "address-field",
      text: "http://localhost:18180",
      description: "Server address",
      className: "android.widget.EditText",
      enabled: true,
      bounds: { left: 42, top: 456, right: 1038, bottom: 582 },
    });
    expect(nodeById(nodes, "address-connect")?.enabled).toBe(false);
    expect(nodeById(nodes, "host-bar")?.description).toBe(
      "Server localhost:18180, not encrypted, test build",
    );
    expect(nodeById(nodes, "nothing")).toBeUndefined();
  });

  test("the entities of XML are read back", () => {
    expect(nodeById(nodes, "odd")?.text).toBe("Tom & Jerry's <shop> › A");
    expect(nodeById(nodes, "odd")?.bounds.left).toBe(-5);
  });

  test("a text is found as a part of what a node shows", () => {
    expect(hasText(nodes, "localhost:18180")).toBe(true);
    expect(hasText(nodes, "Connect")).toBe(true);
    // A description is not a text on the screen.
    expect(hasText(nodes, "not encrypted")).toBe(false);
  });

  test("the point to tap is the middle of the bounds", () => {
    expect(centre({ left: 42, top: 2169, right: 1038, bottom: 2295 })).toEqual({ x: 540, y: 2232 });
    expect(centre({ left: 0, top: 0, right: 3, bottom: 3 })).toEqual({ x: 2, y: 2 });
  });

  test("a text that is no dump has no nodes", () => {
    expect(parseUiDump("ERROR: could not get idle state.")).toEqual([]);
  });
});

describe("what adb says about the device", () => {
  test("the DevTools sockets of WebViews, once each", () => {
    const unix = [
      "Num       RefCount Protocol Flags    Type St Inode Path",
      "0000000000000000: 00000002 00000000 00010000 0001 01 27113 @webview_devtools_remote_5881",
      "0000000000000000: 00000003 00000000 00000000 0001 03 27990 @webview_devtools_remote_5881",
      "0000000000000000: 00000002 00000000 00010000 0001 01  9911 @chrome_devtools_remote",
      "0000000000000000: 00000002 00000000 00010000 0001 01 30001 @webview_devtools_remote_612",
      "0000000000000000: 00000002 00000000 00010000 0001 01 30002 /dev/socket/zygote",
    ].join("\n");
    expect(devtoolsSockets(unix)).toEqual([
      { name: "webview_devtools_remote_5881", pid: 5881 },
      { name: "webview_devtools_remote_612", pid: 612 },
    ]);
    expect(devtoolsSockets("")).toEqual([]);
  });

  test("the package of the activity on top", () => {
    expect(
      resumedPackage(
        "  topResumedActivity=ActivityRecord{266072824 u0 com.google.android.apps.nexuslauncher/.NexusLauncherActivity t6}\n",
      ),
    ).toBe("com.google.android.apps.nexuslauncher");
    expect(
      resumedPackage(
        "    mResumedActivity: ActivityRecord{9f1c2d u0 org.obop.app.test/.MainActivity t12}\n",
      ),
    ).toBe("org.obop.app.test");
    // The line of the whole display wins over the line of one task.
    expect(
      resumedPackage(
        "  mResumedActivity: ActivityRecord{1 u0 org.obop.app.test/.MainActivity t12}\n  topResumedActivity=ActivityRecord{2 u0 com.android.chrome/com.google.android.apps.chrome.Main t13}\n",
      ),
    ).toBe("com.android.chrome");
    expect(resumedPackage("nothing here")).toBeUndefined();
  });

  test("the package of the home screen", () => {
    expect(
      homePackage(
        "priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\ncom.google.android.apps.nexuslauncher/.NexusLauncherActivity\n",
      ),
    ).toBe("com.google.android.apps.nexuslauncher");
    expect(homePackage("No activity found\n")).toBeUndefined();
  });
});

describe("what the script types and reads", () => {
  test("an address is typed in quotes, and only an address", () => {
    expect(inputTextArgument("http://localhost:18181")).toBe("'http://localhost:18181'");
    expect(inputTextArgument("https://[::1]:8443/back-office_2")).toBe(
      "'https://[::1]:8443/back-office_2'",
    );
    for (const text of ["", "a b", "x'; reboot; '", "$(id)", "a&b", "a|b", "`id`", "a\nb"]) {
      expect(() => inputTextArgument(text), text).toThrow();
    }
  });

  test("the account file", () => {
    expect(parseAccount("user=dave\npassword=a=b c\n")).toEqual({
      user: "dave",
      password: "a=b c",
    });
    expect(parseAccount("user=dave\r\npassword=secret\r\n")).toEqual({
      user: "dave",
      password: "secret",
    });
    expect(parseAccount("user=dave\n")).toBeUndefined();
    expect(parseAccount("")).toBeUndefined();
  });

  test("the page of the instance among the DevTools targets, by its origin", () => {
    const page = {
      type: "page",
      url: "http://localhost:18180/#/crm/lead",
      webSocketDebuggerUrl: "ws://127.0.0.1:9333/devtools/page/A",
    };
    const targets = [
      {
        type: "service_worker",
        url: "http://localhost:18180/sw.js",
        webSocketDebuggerUrl: "ws://x",
      },
      { type: "page", url: "http://localhost:181800/", webSocketDebuggerUrl: "ws://y" },
      { type: "page", url: "about:blank", webSocketDebuggerUrl: "ws://z" },
      page,
    ];
    expect(instancePage(targets, "http://localhost:18180")).toBe(page);
    expect(instancePage(targets, "http://localhost:1818")).toBeUndefined();
    expect(instancePage({ not: "a list" }, "http://localhost:18180")).toBeUndefined();
    expect(instancePage([null, 3, { type: "page" }], "http://localhost:18180")).toBeUndefined();
  });
});
