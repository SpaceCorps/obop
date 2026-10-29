// Drives the Mileage app in the iOS Simulator the way a person does: signs in through the system
// browser sheet, starts a trip, goes to the Home screen and back, stops it, submits it. It is a
// UI-testing bundle (XCUITest) built by scripts/simulator/ui/make-project.rb and run by
// scripts/simulator/ui/drive.mjs, one test method for each step, because the route is fed to the
// Simulator from outside (`xcrun simctl location`) between the steps and a test process inside the
// Simulator cannot do that.
//
// It reads, from the environment of `xcodebuild` (variables with the prefix TEST_RUNNER_):
//   OBOP_APP_BUNDLE_ID        bundle id of the installed app (default org.obop.mileage.test)
//   OBOP_DRIVER_USER          user name for the sign-in page of the instance
//   OBOP_DRIVER_PASSWORD      its password (typed into the page; never printed or put in a screenshot)
//   OBOP_SHOT_DIR             a directory for screenshots (PNG)
//
// docs/spec/mobile.md 8.8. A test tool for the Mac; it is not part of the app, its build or any image.
import XCTest

final class MileageDrive: XCTestCase {
  let env = ProcessInfo.processInfo.environment
  var bundleId: String { env["OBOP_APP_BUNDLE_ID"] ?? "org.obop.mileage.test" }
  lazy var app = XCUIApplication(bundleIdentifier: bundleId)
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")

  override func setUp() {
    continueAfterFailure = false
  }

  // MARK: helpers

  func shot(_ name: String) {
    let png = XCUIScreen.main.screenshot().pngRepresentation
    if let dir = env["OBOP_SHOT_DIR"] {
      try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
    }
  }

  /// The labels on screen, for the log when a step fails. Never the value of a text field.
  func dump(_ why: String) {
    print("DUMP \(why)")
    for query in [app.staticTexts, app.buttons] {
      for i in 0..<min(query.count, 40) {
        let e = query.element(boundBy: i)
        if e.exists { print("  \(e.elementType.rawValue) \(e.identifier) / \(e.label)") }
      }
    }
    if springboard.alerts.firstMatch.exists { print("  SYSTEM ALERT: \(springboard.alerts.firstMatch.label)") }
  }

  func waitFor(_ element: XCUIElement, _ seconds: TimeInterval, _ what: String) {
    if !element.waitForExistence(timeout: seconds) {
      shot("failed-\(what.replacingOccurrences(of: " ", with: "-"))")
      dump("waiting for \(what)")
      XCTFail("did not find \(what) within \(Int(seconds)) s")
    }
  }

  /// Taps the "While Using" button of a system location prompt, if one is up.
  @discardableResult
  func answerLocationPrompt(timeout: TimeInterval) -> Bool {
    let alert = springboard.alerts.firstMatch
    guard alert.waitForExistence(timeout: timeout) else { return false }
    print("SYSTEM ALERT: \(alert.label)")
    for b in alert.buttons.allElementsBoundByIndex { print("  ALERT BUTTON: \(b.label)") }
    shot("location-prompt")
    let allow = alert.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'While Using'")).firstMatch
    if allow.exists {
      allow.tap()
      return true
    }
    return false
  }

  /// Types into a field of the sign-in page. A tap on a field of a web page does not always give it
  /// the keyboard at once (seen with the password field: "Neither element nor any descendant has
  /// keyboard focus"), so the tap is repeated until the field has the focus. The text is typed
  /// once, and XCTest does not log it for a secure field.
  func type(into field: XCUIElement, _ text: String, _ what: String) {
    var focused = false
    for attempt in 1...5 {
      // The keyboard of the username field puts its suggestion bar over the lower half of the
      // password field, and the centre of the field takes the tap: the later attempts tap the
      // top edge of the field, and the last ones scroll the page up first.
      if attempt == 1 {
        field.tap()
      } else {
        if attempt >= 4 { app.webViews.firstMatch.swipeUp() }
        field.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.12)).tap()
      }
      sleep(1)
      if (field.value(forKey: "hasKeyboardFocus") as? Bool) == true {
        focused = true
        break
      }
    }
    if !focused { print("NO-KEYBOARD-FOCUS: \(what)") }
    shot("typing-\(what)")
    field.typeText(text)
  }

  /// The State row of the trip page. A row reads as one element, "State, Ended", so the state is
  /// looked for in the label of an element that begins with "State".
  func stateRow(_ state: String) -> XCUIElement {
    app.descendants(matching: .any)
      .matching(NSPredicate(format: "label BEGINSWITH 'State' AND label CONTAINS[c] %@", state))
      .firstMatch
  }

  func readout(_ id: String) -> String {
    let e = app.staticTexts[id]
    return e.exists ? e.label : "(not shown)"
  }

  // MARK: steps

  /// Signs in through the system browser sheet (ASWebAuthenticationSession), as the driver.
  func signIn() {
    let address = app.textFields["signin-address"]
    waitFor(address, 20, "the Sign in screen")
    print("SERVER-ADDRESS: \(address.value as? String ?? "")")
    shot("signin-1-screen")
    app.buttons["signin-button"].tap()
    let cont = springboard.alerts.firstMatch.buttons["Continue"]
    waitFor(cont, 20, "the system sign-in alert")
    print("SYSTEM ALERT: \(springboard.alerts.firstMatch.label)")
    shot("signin-2-system-alert")
    cont.tap()
    let user = app.textFields["Username"]
    waitFor(user, 30, "the sign-in page of the instance")
    type(into: user, env["OBOP_DRIVER_USER"] ?? "", "user")
    let password = app.secureTextFields["Password"]
    type(into: password, env["OBOP_DRIVER_PASSWORD"] ?? "", "password")
    shot("signin-3-page")
    app.webViews.buttons["Sign in"].firstMatch.tap()
    for second in [1, 3, 6] {
      sleep(UInt32(second == 1 ? 1 : second - (second == 3 ? 1 : 3)))
      shot("signin-3b-after-tap-\(second)s")
    }
    let allow = app.webViews.buttons["Allow"].firstMatch
    waitFor(allow, 30, "the consent page")
    shot("signin-4-consent")
    allow.tap()
    waitFor(app.buttons["start"], 60, "Home after the sign-in")
    shot("signin-5-home")
  }

  func test01_SignIn() {
    // A fresh process: an earlier aborted run may have left the app on "Signing in".
    app.launch()
    if app.buttons["start"].waitForExistence(timeout: 3) {
      print("ALREADY-SIGNED-IN")
      return
    }
    signIn()
    print("HOME-REACHED")
  }

  /// The Sign in screen is what a fresh install must show (the Keychain still holds the old tokens).
  func test10_ExpectSignInScreen() {
    app.activate()
    waitFor(app.textFields["signin-address"], 30, "the Sign in screen")
    shot("fresh-install-signin")
    print("SIGNIN-SCREEN-SHOWN")
  }

  /// After `xcrun simctl keychain UDID reset` and a sign-in: the app replaced the database file that
  /// no key fitted and says so on Home (docs/spec/mobile.md 8.8). Run as `test01_SignIn,test11_ExpectRestoredBanner`.
  func test11_ExpectRestoredBanner() {
    app.activate()
    waitFor(app.buttons["start"], 20, "Home")
    let banner = app.descendants(matching: .any).matching(identifier: "restored-banner").firstMatch
    waitFor(banner, 10, "the notice about the replaced database file")
    shot("restored-banner")
    print("RESTORED-BANNER-SHOWN")
  }

  func test02_Start() {
    app.activate()
    waitFor(app.buttons["start"], 20, "Home")
    // The purpose buttons are not plain buttons for the accessibility tree: ask for any element.
    let business = app.descendants(matching: .any).matching(identifier: "purpose-business").firstMatch
    if business.exists { business.tap() }
    app.buttons["start"].tap()
    let cont = app.buttons["disclosure-continue"]
    if cont.waitForExistence(timeout: 10) {
      shot("start-1-disclosure")
      cont.tap()
    }
    if answerLocationPrompt(timeout: 15) { print("LOCATION-PROMPT-ANSWERED") }
    let guide = app.buttons["guide-start"]
    if guide.waitForExistence(timeout: 15) {
      shot("start-2-guide")
      guide.tap()
    }
    waitFor(app.buttons["stop"], 30, "the Recording screen")
    shot("start-3-recording")
    print("RECORDING-SHOWN")
  }

  /// Presses the Home button: the app goes to the background and must go on recording.
  /// Presses the Home button while the route is moving: the app goes to the background and must go
  /// on recording (UIBackgroundModes location, "When In Use").
  func test03_Background() {
    app.activate()
    waitFor(app.buttons["stop"], 20, "the Recording screen")
    sleep(15)
    print("READOUT-BEFORE-HOME: \(counters())")
    XCUIDevice.shared.press(.home)
    print("HOME-PRESSED")
    sleep(45)
    shot("background-1-springboard")
    print("APP-STATE-IN-BACKGROUND: \(app.state.rawValue)")
  }

  /// Back to the app, then waits until the route is over: the points stop growing.
  func test04_Foreground() {
    app.activate()
    waitFor(app.buttons["stop"], 20, "the Recording screen")
    print("READOUT-AFTER-BACKGROUND: \(counters())")
    shot("foreground-1-recording")
    var last = readout("points")
    var quiet = 0
    for _ in 1...60 {  // up to ten minutes
      sleep(10)
      let now = readout("points")
      print("READOUT: \(counters())")
      quiet = now == last ? quiet + 1 : 0
      last = now
      if quiet >= 3 { break }  // 30 s without a new point: the route is over
    }
    shot("foreground-2-route-done")
    print("ROUTE-DONE: \(counters())")
  }

  func test05_Stop() {
    app.activate()
    let stop = app.buttons["stop"]
    waitFor(stop, 20, "the Recording screen")
    print("READOUT-AT-STOP: \(counters())")
    stop.tap()
    let confirm = app.alerts.firstMatch.buttons["Stop"]
    waitFor(confirm, 10, "the Stop confirmation")
    shot("stop-1-confirm")
    confirm.tap()
    waitFor(stateRow("Ended"), 180, "the state Ended")
    shot("stop-2-ended")
    print("TRIP-ENDED")
  }

  func test06_Submit() {
    app.activate()
    let submit = app.buttons["submit"]
    waitFor(submit, 20, "the Submit button")
    submit.tap()
    waitFor(stateRow("In review"), 60, "the state In review")
    shot("submit-1-in-review")
    print("TRIP-IN-REVIEW")
  }

  /// Stops the trip that is being recorded (if one is) and discards it, with a reason.
  func test20_StopAndDiscard() {
    app.activate()
    let stop = app.buttons["stop"]
    if stop.waitForExistence(timeout: 5) {
      stop.tap()
      let confirm = app.alerts.firstMatch.buttons["Stop"]
      waitFor(confirm, 10, "the Stop confirmation")
      confirm.tap()
    }
    // The form may already be open (an earlier run stopped half way).
    if !app.buttons["discard-confirm"].waitForExistence(timeout: 3) {
      let discard = app.buttons["discard"]
      waitFor(discard, 60, "the Discard button")
      discard.tap()
    }
    // A multi-line field is a text view, not a text field: ask for any element.
    let reason = app.descendants(matching: .any).matching(identifier: "discard-reason").firstMatch
    waitFor(reason, 10, "the reason field")
    reason.tap()
    reason.typeText("Simulator test trip")
    app.buttons["discard-confirm"].tap()
    waitFor(app.buttons["start"], 30, "Home after the discard")
    shot("discard-1-home")
    print("TRIP-DISCARDED")
  }

  func counters() -> String {
    "elapsed=\(readout("elapsed")) distance=\(readout("distance").prefix(12)) points=\(readout("points").prefix(4))"
  }
}
