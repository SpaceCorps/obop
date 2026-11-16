// Drives the host app in the iOS Simulator the way a person does, step by step, from a short script
// of actions: the address screen, the page's own sign-in, the back office, "This app", Change
// server, rotation. `simctl` cannot tap, so the taps are XCUITest's. A UI-testing bundle built by
// scripts/simulator/make-project.rb and run by scripts/simulator/drive.mjs (docs/spec/mobile-host.md
// 12.8). A test tool for the Mac; it is not part of the app, its build or any image.
//
// It reads, from the environment of `xcodebuild` (variables with the prefix TEST_RUNNER_):
//   OBOP_APP_BUNDLE_ID     bundle id of the installed app (default org.obop.app.test)
//   OBOP_SCRIPT            the actions, one per line (scripts/simulator/drive-lib.mjs, ACTIONS)
//   OBOP_DRIVER_USER       typed for $USER
//   OBOP_DRIVER_PASSWORD   typed for $PASSWORD (never printed; a screenshot never shows it)
//   OBOP_SHOT_DIR          a directory for the screenshots (PNG)
import XCTest

final class HostDrive: XCTestCase {
  let env = ProcessInfo.processInfo.environment
  var bundleId: String { env["OBOP_APP_BUNDLE_ID"] ?? "org.obop.app.test" }
  lazy var app = XCUIApplication(bundleIdentifier: bundleId)
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")

  override func setUp() {
    continueAfterFailure = false
  }

  func shot(_ name: String) {
    let png = XCUIScreen.main.screenshot().pngRepresentation
    if let dir = env["OBOP_SHOT_DIR"] {
      try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
    }
    print("SHOT \(name)")
  }

  /// The labels on screen, for the log. Never the value of a text field.
  func dump(_ why: String) {
    print("DUMP \(why)")
    for query in [app.staticTexts, app.buttons, app.links, app.textFields, app.secureTextFields, app.otherElements.matching(NSPredicate(format: "label != ''"))] {
      for i in 0..<min(query.count, 60) {
        let e = query.element(boundBy: i)
        if e.exists {
          let f = e.frame
          print("  \(e.elementType.rawValue) id=\(e.identifier) label=\(e.label) frame=\(Int(f.minX)),\(Int(f.minY)),\(Int(f.width))x\(Int(f.height))")
        }
      }
    }
    if springboard.alerts.firstMatch.exists { print("  SYSTEM ALERT: \(springboard.alerts.firstMatch.label)") }
  }

  /// The first element whose label or identifier is `text`, else whose label contains it.
  func find(_ text: String, in host: XCUIApplication? = nil) -> XCUIElement {
    let root = host ?? app
    let exact = root.descendants(matching: .any)
      .matching(NSPredicate(format: "label == %@ OR identifier == %@", text, text)).firstMatch
    if exact.exists { return exact }
    return root.descendants(matching: .any)
      .matching(NSPredicate(format: "label CONTAINS[c] %@", text)).firstMatch
  }

  func field(_ spec: String) -> XCUIElement {
    // "secure:N", "text:N" or the label / placeholder of the field.
    let parts = spec.split(separator: ":", maxSplits: 1).map(String.init)
    if parts.count == 2, let n = Int(parts[1]) {
      let query = parts[0] == "secure" ? app.secureTextFields : app.textFields
      return query.element(boundBy: n)
    }
    let byLabel = app.descendants(matching: .any).matching(
      NSPredicate(format: "(elementType == %d OR elementType == %d) AND (label == %@ OR placeholderValue == %@ OR identifier == %@)",
                  XCUIElement.ElementType.textField.rawValue, XCUIElement.ElementType.secureTextField.rawValue,
                  spec, spec, spec)).firstMatch
    return byLabel
  }

  func value(_ text: String) -> String {
    switch text {
    case "$USER": return env["OBOP_DRIVER_USER"] ?? ""
    case "$PASSWORD": return env["OBOP_DRIVER_PASSWORD"] ?? ""
    default: return text
    }
  }

  func focus(_ f: XCUIElement) {
    for attempt in 1...5 {
      if attempt == 1 { f.tap() } else { f.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap() }
      sleep(1)
      if (f.value(forKey: "hasKeyboardFocus") as? Bool) == true { return }
    }
    print("NO-KEYBOARD-FOCUS")
  }

  func testScript() {
    let script = env["OBOP_SCRIPT"] ?? ""
    for raw in script.split(separator: "\n") {
      let line = raw.trimmingCharacters(in: .whitespaces)
      if line.isEmpty || line.hasPrefix("#") { continue }
      let words = line.split(separator: " ", maxSplits: 1).map(String.init)
      let action = words[0]
      let arg = words.count > 1 ? words[1] : ""
      print("STEP \(action) \(arg.contains("$PASSWORD") ? "(secret)" : arg)")
      switch action {
      case "launch":
        app.terminate()
        app.launch()
      case "activate":
        app.activate()
      case "terminate":
        app.terminate()
      case "sleep":
        sleep(UInt32(arg) ?? 1)
      case "shot":
        shot(arg)
      case "dump":
        dump(arg)
      case "wait", "waitsb":
        // wait <seconds> <text>
        let p = arg.split(separator: " ", maxSplits: 1).map(String.init)
        let e = find(p[1], in: action == "waitsb" ? springboard : nil)
        if !e.waitForExistence(timeout: Double(p[0]) ?? 10) {
          shot("failed-wait")
          dump("waiting for \(p[1])")
          XCTFail("did not find \(p[1]) within \(p[0]) s")
          return
        }
      case "waitgone":
        let p = arg.split(separator: " ", maxSplits: 1).map(String.init)
        let e = find(p[1])
        let gone = e.waitForNonExistence(timeout: Double(p[0]) ?? 10)
        print("WAITGONE \(p[1]) \(gone)")
      case "exists":
        print("EXISTS \(arg) \(find(arg).exists)")
      case "tap", "tapsb":
        let e = find(arg, in: action == "tapsb" ? springboard : nil)
        if !e.waitForExistence(timeout: 10) {
          shot("failed-tap")
          dump("tap \(arg)")
          XCTFail("nothing to tap: \(arg)")
          return
        }
        e.tap()
      case "tapif":
        let e = find(arg)
        if e.exists { e.tap(); print("TAPPED \(arg)") } else { print("NOT-THERE \(arg)") }
      case "tapxy":
        // tapxy 0.5 0.1: a point of the window, as fractions of its size
        let p = arg.split(separator: " ").compactMap { Double($0) }
        app.coordinate(withNormalizedOffset: CGVector(dx: p[0], dy: p[1])).tap()
      case "type":
        // type <field> <text>
        let p = arg.split(separator: " ", maxSplits: 1).map(String.init)
        let f = field(p[0])
        if !f.waitForExistence(timeout: 15) {
          shot("failed-field")
          dump("field \(p[0])")
          XCTFail("no field \(p[0])")
          return
        }
        focus(f)
        f.typeText(value(p.count > 1 ? p[1] : ""))
      case "clear":
        let f = field(arg)
        focus(f)
        let current = f.value as? String ?? ""
        f.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count))
      case "key":
        // A key of the software keyboard by its label (Go, Return, ...).
        let k = app.keyboards.buttons[arg]
        if k.waitForExistence(timeout: 5) { k.tap() } else { print("NO-KEY \(arg)"); dump("key") }
      case "keyboard":
        print("KEYBOARD \(app.keyboards.firstMatch.exists ? "shown frame=\(app.keyboards.firstMatch.frame)" : "hidden")")
      case "frame":
        let e = find(arg)
        print("FRAME \(arg) \(e.exists ? "\(e.frame)" : "absent")")
      case "orient":
        XCUIDevice.shared.orientation = arg == "landscape" ? .landscapeLeft : .portrait
        sleep(2)
      case "home":
        XCUIDevice.shared.press(.home)
      case "swipeback":
        // The edge pan of the WebView: from the very edge, slowly, held at the end.
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.0, dy: 0.5))
        start.press(forDuration: 0.1, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)),
                    withVelocity: 500, thenHoldForDuration: 0.1)
      case "swipeup":
        app.swipeUp()
      case "swipedown":
        app.swipeDown()
      case "notnow":
        // The system offers to save a password after a sign-in: a test never saves one.
        for host in [app, springboard] {
          let b = host.descendants(matching: .any).matching(NSPredicate(format: "label == 'Not Now'")).firstMatch
          if b.waitForExistence(timeout: 3) { b.tap(); print("SAVE-PASSWORD-DISMISSED"); break }
        }
      case "state":
        print("APP-STATE \(app.state.rawValue)")
      default:
        XCTFail("unknown action \(action)")
        return
      }
    }
    print("SCRIPT-DONE")
  }
}
