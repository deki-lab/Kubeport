---
status: superseded by ADR-0003
---

# Tauri v2 + kube-rs, without kubectl or SwiftUI

The app was meant to be written in Rust end to end: Tauri v2 for the menu bar icon and an HTML panel anchored below it, `kube-rs` to talk to the Kubernetes API directly. Dropped before any implementation once the "minimal memory + instant panel" constraint appeared: the WebKit webview costs ~60–100 MB if kept alive, and 200–500 ms on every open if destroyed.

## Considered Options

- **SwiftUI `MenuBarExtra` + `kubectl` as a subprocess**: fully native rendering, but depends on `kubectl` and needs fragile process supervision.
- **Pure Rust (`tray-icon` + `muda`)**: a real native menu, but a form does not fit in it: it needs an egui/iced window that does not look native.
- **SwiftUI + Rust core via UniFFI**: chosen later, see ADR-0003.
