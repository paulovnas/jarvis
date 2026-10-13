import Cocoa

// Disposable native fixture: its only effects are dummy text and local test receipts.
final class FixtureDelegate: NSObject, NSApplicationDelegate, NSTextFieldDelegate {
    private var window: NSWindow!
    private var input: NSTextField!
    private var result: NSTextField!
    private var clicks = 0
    private var timer: Timer?
    private let receipt = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "/tmp/jarvis-computer-fixture.json")

    func applicationDidFinishLaunching(_ notification: Notification) {
        window = NSWindow(contentRect: NSRect(x: 400, y: 240, width: 580, height: 300),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.title = "Jarvis Computer Use Fixture"
        let content = window.contentView!
        let heading = NSTextField(labelWithString: "Somente dados fictícios")
        heading.frame = NSRect(x: 24, y: 240, width: 480, height: 24)
        content.addSubview(heading)
        input = NSTextField(frame: NSRect(x: 24, y: 184, width: 520, height: 32))
        input.stringValue = "fixture"
        input.setAccessibilityLabel("Entrada fictícia")
        input.delegate = self
        content.addSubview(input)
        let button = NSButton(title: "Confirmar teste", target: self, action: #selector(confirm))
        button.frame = NSRect(x: 24, y: 126, width: 180, height: 32)
        content.addSubview(button)
        result = NSTextField(labelWithString: "Aguardando teste")
        result.frame = NSRect(x: 24, y: 72, width: 520, height: 32)
        content.addSubview(result)
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        publish()
        timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            guard let self else { return }
            let command = receipt.deletingLastPathComponent().appendingPathComponent("command.json")
            guard let data = try? Data(contentsOf: command),
                  let value = try? JSONSerialization.jsonObject(with: data) as? [String: String] else { return }
            try? FileManager.default.removeItem(at: command)
            if value["action"] == "move" {
                window.setFrameOrigin(NSPoint(x: window.frame.origin.x + 44, y: window.frame.origin.y + 24))
            }
            publish()
        }
    }

    @objc private func confirm() {
        clicks += 1
        result.stringValue = "Resultado fictício: \(input.stringValue)"
        publish()
    }

    func controlTextDidChange(_ notification: Notification) { publish() }

    private func publish() {
        let state: [String: Any] = ["text": input.stringValue, "clicks": clicks,
                                    "result": result.stringValue,
                                    "windowX": window.frame.origin.x, "windowY": window.frame.origin.y]
        guard let data = try? JSONSerialization.data(withJSONObject: state, options: [.sortedKeys]) else { return }
        try? data.write(to: receipt, options: .atomic)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

let app = NSApplication.shared
let delegate = FixtureDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
