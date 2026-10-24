// A throwaway prototype, not part of wxread: drives WeChat for Mac's window
// the way a person does (keys, a screenshot, Vision's text recognition, a
// click). docs/leave-group.md says what it found.
//
//   swift docs/leave-group/wxwin.swift run <group id> [--out <dir>] [--confirm]
//       the whole path up to the confirmation, which it cancels; --confirm
//       presses OK instead, and only on the group named "wxread 退群测试".
//       It reads the group with wxread, so WXREAD_KEYS must hold the keys.
//   swift docs/leave-group/wxwin.swift front
//   swift docs/leave-group/wxwin.swift shot <out.png>        window shot + recognized text as JSON
//   swift docs/leave-group/wxwin.swift key <name> [cmd]      return, escape, f, down, …
//   swift docs/leave-group/wxwin.swift type <text>
//   swift docs/leave-group/wxwin.swift click <x> <y>         window points, from the top left
//   swift docs/leave-group/wxwin.swift menu <menu> <item>    press a menu item
//   swift docs/leave-group/wxwin.swift paste <text>          Edit > Paste it, the pasteboard kept
//   swift docs/leave-group/wxwin.swift idle                  seconds since the person last touched the Mac
import AppKit
import Vision

let bundle = "com.tencent.xinWeChat"

func fail(_ s: String) -> Never {
    FileHandle.standardError.write((s + "\n").data(using: .utf8)!)
    exit(1)
}

func app() -> NSRunningApplication {
    guard let a = NSRunningApplication.runningApplications(withBundleIdentifier: bundle).first else {
        fail("WeChat is not running")
    }
    return a
}

// WeChat's main window: its largest on-screen window at layer 0.
func window() -> (id: CGWindowID, frame: CGRect) {
    let pid = app().processIdentifier
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
    var best: (CGWindowID, CGRect)?
    for w in list where (w[kCGWindowOwnerPID as String] as? pid_t) == pid
        && (w[kCGWindowLayer as String] as? Int) == 0 {
        var r = CGRect.zero
        CGRectMakeWithDictionaryRepresentation(w[kCGWindowBounds as String] as! CFDictionary, &r)
        if best == nil || r.width * r.height > best!.1.width * best!.1.height {
            best = (w[kCGWindowNumber as String] as! CGWindowID, r)
        }
    }
    guard let b = best else { fail("WeChat has no window on screen") }
    return b
}

// The confirmation is a window of its own over the main one: its frame in
// the main window's points, or nil when none is up.
func dialog() -> CGRect? {
    let pid = app().processIdentifier
    let (main, f) = window()
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
    for w in list where (w[kCGWindowOwnerPID as String] as? pid_t) == pid
        && (w[kCGWindowNumber as String] as? CGWindowID) != main {
        var r = CGRect.zero
        CGRectMakeWithDictionaryRepresentation(w[kCGWindowBounds as String] as! CFDictionary, &r)
        if f.contains(r) { return r.offsetBy(dx: -f.minX, dy: -f.minY) }
    }
    return nil
}

// NSRunningApplication.isActive is not updated in a process with no run
// loop: Accessibility is asked instead.
func isFront() -> Bool {
    var v: CFTypeRef?
    AXUIElementCopyAttributeValue(AXUIElementCreateApplication(app().processIdentifier),
                                  kAXFrontmostAttribute as CFString, &v)
    return (v as? Bool) ?? false
}

// Makes WeChat the active app. NSRunningApplication.activate() from a
// process in the background is not always honored; Accessibility's
// AXFrontmost is, and it is checked.
func front() {
    for _ in 0..<10 {
        AXUIElementSetAttributeValue(AXUIElementCreateApplication(app().processIdentifier),
                                     kAXFrontmostAttribute as CFString, kCFBooleanTrue)
        usleep(300_000)
        if isFront() { return }
    }
    fail("WeChat did not become the active app")
}

struct Text: Encodable { let text: String; let x: Double; let y: Double; let w: Double; let h: Double }

// The window's shot, and each line Vision reads in it, in window points.
func shot(_ out: String) -> [Text] {
    let (id, frame) = window()
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", "-o", "-l", String(id), out]
    try! p.run(); p.waitUntilExit()
    guard p.terminationStatus == 0, let img = NSImage(contentsOfFile: out),
          let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
        fail("screencapture took no shot of window \(id): is Screen Recording allowed for this app?")
    }
    let req = VNRecognizeTextRequest()
    req.recognitionLevel = .accurate
    req.recognitionLanguages = ["zh-Hans", "en-US"]
    req.usesLanguageCorrection = false
    try! VNImageRequestHandler(cgImage: cg).perform([req])
    return (req.results ?? []).compactMap { o in
        guard let t = o.topCandidates(1).first else { return nil }
        let b = o.boundingBox  // normalized, origin bottom left
        return Text(text: t.string,
                    x: b.minX * frame.width, y: (1 - b.maxY) * frame.height,
                    w: b.width * frame.width, h: b.height * frame.height)
    }
}

let keys: [String: CGKeyCode] = ["return": 36, "escape": 53, "tab": 48, "down": 125, "up": 126,
                                  "f": 3, "a": 0, "v": 9, "delete": 51, "space": 49]

func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap); usleep(30_000) }

func key(_ name: String, cmd: Bool) {
    guard let k = keys[name] else { fail("no key \(name)") }
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: nil, virtualKey: k, keyDown: down)
        if cmd { e?.flags = .maskCommand }
        post(e)
    }
}

func type(_ s: String) {
    for ch in s.utf16 {
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
            var c = ch
            e?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &c)
            post(e)
        }
    }
}

// A click reaches WeChat only while it is the active app: on an inactive
// window the first click only activates it.
func click(_ x: Double, _ y: Double) {
    if !isFront() { front() }
    let f = window().frame
    let p = CGPoint(x: f.minX + x, y: f.minY + y)
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left))
    post(CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left))
    post(CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left))
}

// Presses a menu item through Accessibility: WeChat answers its menus, and a
// pressed item reaches it without a synthetic key that a person's own typing
// could race.
func menu(_ title: String, _ item: String) {
    let ax = AXUIElementCreateApplication(app().processIdentifier)
    func children(_ e: AXUIElement) -> [AXUIElement] {
        var v: CFTypeRef?
        AXUIElementCopyAttributeValue(e, kAXChildrenAttribute as CFString, &v)
        return (v as? [AXUIElement]) ?? []
    }
    func name(_ e: AXUIElement) -> String {
        var v: CFTypeRef?
        AXUIElementCopyAttributeValue(e, kAXTitleAttribute as CFString, &v)
        return (v as? String) ?? ""
    }
    var bar: CFTypeRef?
    AXUIElementCopyAttributeValue(ax, kAXMenuBarAttribute as CFString, &bar)
    guard let bar else { fail("WeChat answers no menu bar") }
    for m in children(bar as! AXUIElement) where name(m) == title {
        for list in children(m) {
            for i in children(list) where name(i) == item {
                let r = AXUIElementPerformAction(i, kAXPressAction as CFString)
                guard r == .success else { fail("pressing \(title) > \(item) answered \(r.rawValue)") }
                return
            }
        }
    }
    fail("WeChat has no menu item \(title) > \(item)")
}

// Puts text where the focus is by Edit > Paste, the pasteboard put back after.
func paste(_ s: String) {
    let pb = NSPasteboard.general
    let kept = pb.pasteboardItems?.map { i in i.types.map { ($0, i.data(forType: $0)) } } ?? []
    pb.clearContents()
    pb.setString(s, forType: .string)
    menu("Edit", "Paste")
    usleep(300_000)
    pb.clearContents()
    let items = kept.map { pairs -> NSPasteboardItem in
        let i = NSPasteboardItem()
        for (t, d) in pairs { if let d { i.setData(d, forType: t) } }
        return i
    }
    if !items.isEmpty { pb.writeObjects(items) }
}

// Seconds since the person at the Mac last touched a key, the mouse or the trackpad.
func idle() -> Double {
    let s = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching("IOHIDSystem"))
    defer { IOObjectRelease(s) }
    let v = IORegistryEntryCreateCFProperty(s, "HIDIdleTime" as CFString, kCFAllocatorDefault, 0)?
        .takeRetainedValue() as? NSNumber
    return (v?.doubleValue ?? 0) / 1e9
}

// MARK: the path, from a group's id to its confirmation

// The one group the prototype may confirm leaving.
let testGroup = "wxread 退群测试"

// Text as Vision reads it, made comparable: no spaces, no quotes, ASCII parentheses.
func norm(_ s: String) -> String {
    var t = s.replacingOccurrences(of: "（", with: "(").replacingOccurrences(of: "）", with: ")")
    t.removeAll { $0.isWhitespace || "\"“”'‘’「」".contains($0) }
    return t
}

func say(_ step: String, _ fields: [String: Any]) {
    var o = fields
    o["step"] = step
    let d = try! JSONSerialization.data(withJSONObject: o, options: [.sortedKeys, .withoutEscapingSlashes])
    print(String(data: d, encoding: .utf8)!)
}

// Runs wxread with the keys the caller supplied (WXREAD_KEYS) and answers its JSON.
func wxread(_ args: [String]) -> [String: Any] {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    p.arguments = ["wxread"] + args
    let pipe = Pipe()
    p.standardOutput = pipe
    try! p.run()
    let d = pipe.fileHandleForReading.readDataToEndOfFile()
    p.waitUntilExit()
    guard let o = try? JSONSerialization.jsonObject(with: d) as? [String: Any] else {
        fail("wxread \(args.joined(separator: " ")) answered no JSON")
    }
    if let e = o["error"] { fail("wxread \(args.joined(separator: " ")): \(e)") }
    return o
}

func scroll(_ x: Double, _ y: Double, lines: Int32) {
    let f = window().frame
    let p = CGPoint(x: f.minX + x, y: f.minY + y)
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left))
    post(CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: lines, wheel2: 0, wheel3: 0))
}

func run(id: String, confirm: Bool, out: String) {
    try! FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)
    var n = 0
    func look(_ name: String) -> [Text] {
        n += 1
        let path = "\(out)/\(n)-\(name).png"
        let t = shot(path)
        say("shot", ["file": path])
        return t
    }
    func stop(_ why: String, _ fields: [String: Any] = [:]) -> Never {
        say("refused", fields.merging(["why": why]) { a, _ in a })
        exit(2)
    }

    // 0. What wxread answers of the group, and that no other group shares its name.
    let g = wxread(["group", id])["group"] as! [String: Any]
    let shown = g["display_name"] as! String
    let own = g["name"] as? String ?? ""
    let members = (g["members"] as! [Any]).count
    // Every contact, page by page: --query matches a contact's own name, not
    // the display name a group with none is shown by.
    var same: [String] = []
    var cursor: String?
    repeat {
        let page = wxread(["contacts", "--limit", "1000"] + (cursor.map { ["--cursor", $0] } ?? []))
        same += (page["contacts"] as! [[String: Any]])
            .filter { $0["kind"] as? String == "group" && $0["display_name"] as? String == shown }
            .map { $0["id"] as! String }
        cursor = page["next"] as? String
    } while cursor != nil
    say("group", ["id": id, "display_name": shown, "name": own, "members": members, "same_name": same])
    if same != [id] { stop("more than one group is shown as \(shown): the window cannot tell them apart", ["groups": same]) }
    let isTest = shown == testGroup && own == testGroup
    if confirm && !isTest { stop("only the group named exactly \(testGroup) may be left by the prototype") }

    // 1. Wait for the person to leave the Mac alone, at most a minute.
    var waited = 0
    while idle() < 3 {
        if waited >= 60 { stop("the Mac was in use for a minute: nothing was touched") }
        sleep(1); waited += 1
    }
    // The app the person was in gets the front back at the end.
    let before = NSWorkspace.shared.frontmostApplication?.processIdentifier
    defer {
        if let before {
            AXUIElementSetAttributeValue(AXUIElementCreateApplication(before),
                                         kAXFrontmostAttribute as CFString, kCFBooleanTrue)
        }
    }
    front()

    // 2. Search for the group by its display name.
    key("escape", cmd: false)
    menu("Edit", "Search")
    usleep(300_000)
    paste(shown)
    usleep(800_000)
    let typed = look("search")
    let field = typed.filter { $0.y < 40 && $0.x < 330 }.map { norm($0.text) }
    say("search", ["field": field])
    // Vision reads the search icon before the text and the clear button after it.
    guard field.contains(where: { $0.contains(norm(shown)) }) else {
        stop("the search box does not hold the name: Return could send it into a chat")
    }
    key("return", cmd: false)
    usleep(1_200_000)

    // 3. The opened chat's title is its name and member count.
    let wantTitle = norm(shown) + "(\(members))"
    let opened = look("chat")
    let title = opened.filter { $0.y < 40 && $0.x > 360 }.map { norm($0.text) }
    say("chat", ["title": title, "want": wantTitle])
    guard title.contains(wantTitle) else { stop("the opened chat is not the group asked for", ["title": title]) }

    // 4. Its chat info, the ··· at the title row's right end; Leave at the panel's foot.
    let w = Double(window().frame.width)
    func infoOpen(_ t: [Text]) -> Bool { t.contains { ["Group Name", "群聊名称"].contains($0.text) && $0.x > w - 300 } }
    var info = opened
    if !infoOpen(opened) {  // the ··· toggles it: it may be open already
        click(w - 34, 27)
        usleep(1_000_000)
        info = look("info")
    }
    guard infoOpen(info) else {
        stop("the chat info did not open")
    }
    var leave = info.first { ["Leave", "退出群聊"].contains($0.text) && $0.x > w - 300 }
    var scrolls = 0
    while leave == nil && scrolls < 8 {
        scroll(w - 150, 600, lines: -10)
        usleep(500_000)
        scrolls += 1
        info = look("info-scrolled")
        leave = info.first { ["Leave", "退出群聊"].contains($0.text) && $0.x > w - 300 }
    }
    let infoTitle = info.filter { $0.y < 40 && $0.x > 360 }.map { norm($0.text) }
    say("info", ["leave": leave.map { [$0.x, $0.y] } as Any, "scrolls": scrolls, "title": infoTitle])
    guard let leave else { stop("no Leave in the chat info") }
    guard infoTitle.contains(wantTitle) else { stop("the chat changed under the info panel", ["title": infoTitle]) }
    click(leave.x + leave.w / 2, leave.y + leave.h / 2)
    usleep(1_000_000)

    // 5. The confirmation names the group; its buttons are found by their text.
    let all = look("confirm")
    guard let frame = dialog() else { stop("no confirmation came up") }
    let inDialog = all.filter { frame.contains(CGPoint(x: $0.x + $0.w / 2, y: $0.y + $0.h / 2)) }
    guard let ask = inDialog.first(where: { $0.text.contains("leave the group chat") || $0.text.contains("退出群聊") }) else {
        stop("no confirmation came up")
    }
    let cancel = inDialog.first { ["Cancel", "取消"].contains($0.text) && $0.y > ask.y }
    let ok = inDialog.first { ["OK", "确定"].contains($0.text) && $0.y > ask.y }
    // The quoted name, on as many lines as it wraps to, down to the
    // Clear Chat History box.
    let box = inDialog.first { $0.y > ask.y && ($0.text.contains("Clear Chat History") || $0.text.contains("聊天记录")) }
    let quoted = inDialog
        .filter { $0.y > ask.y + ask.h / 2 && $0.y < (box?.y ?? cancel?.y ?? ask.y + 80) - 2 }
        .sorted { ($0.y, $0.x) < ($1.y, $1.x) }
        .map { norm($0.text) }
    say("confirm", ["asks": ask.text, "names": quoted, "cancel": cancel.map { [$0.x, $0.y] } as Any, "ok": ok.map { [$0.x, $0.y] } as Any])
    guard let cancel, let ok else { stop("the confirmation's buttons were not read") }
    let named = quoted.joined() == norm(shown)
    if !(confirm && isTest && named) {
        click(cancel.x + cancel.w / 2, cancel.y + cancel.h / 2)
        usleep(800_000)
        let after = look("cancelled")
        let gone = dialog() == nil && !after.contains { $0.text.contains("leave the group chat") }
        say("cancelled", ["dialog_gone": gone, "named": named])
        key("escape", cmd: false)
        return
    }

    // 6. Only on the test group: confirm, then read the databases until it shows.
    click(ok.x + ok.w / 2, ok.y + ok.h / 2)
    let clicked = Date()
    usleep(800_000)
    _ = look("left")
    say("confirmed", ["at": ISO8601DateFormatter().string(from: clicked)])
    for _ in 0..<60 {
        let still = (wxread(["group", id])["group"] as! [String: Any])["members"] as! [[String: Any]]
        if still.count < members {
            say("membership", ["members": still.map { $0["id"]! }, "seconds": Date().timeIntervalSince(clicked)])
            return
        }
        sleep(1)
    }
    say("membership", ["members": members, "seconds": Date().timeIntervalSince(clicked), "unchanged": true])
}

let a = CommandLine.arguments
guard a.count >= 2 else { fail("usage: front | shot <out.png> | key <name> [cmd] | type <text> | click <x> <y>") }
if !["shot", "idle"].contains(a[1]) && !AXIsProcessTrusted() {
    fail("Accessibility is not allowed for this app: keys and clicks would be dropped")
}
switch a[1] {
case "front": front(); print(window().frame)
case "shot":
    let t = shot(a[2])
    let enc = JSONEncoder(); enc.outputFormatting = [.withoutEscapingSlashes]
    print(String(data: try! enc.encode(["window": [window().frame.width, window().frame.height]]), encoding: .utf8)!)
    for x in t { print(String(data: try! enc.encode(x), encoding: .utf8)!) }
case "key": key(a[2], cmd: a.count > 3)
case "type": type(a[2])
case "run":
    let out = a.firstIndex(of: "--out").map { a[$0 + 1] } ?? "leave-group-run"
    run(id: a[2], confirm: a.contains("--confirm"), out: out)
case "menu": menu(a[2], a[3])
case "paste": paste(a[2])
case "idle": print(idle())
case "click": click(Double(a[2])!, Double(a[3])!)
default: fail("no command \(a[1])")
}
