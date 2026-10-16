// On macOS, run from a complete .app with its own CEF framework and helpers;
// CEF's sandbox rejects framework symlinks pointing outside that bundle.
fn main() {
    jarvis_lib::run_browser_probe();
}
