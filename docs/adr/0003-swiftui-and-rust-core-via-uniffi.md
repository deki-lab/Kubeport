# SwiftUI interface, Rust core via UniFFI

The interface is a SwiftUI `MenuBarExtra` app; all Kubernetes logic (kubeconfig, `exec` auth, port-forward, resolving and switching the Resolved pod, TOML config) lives in a Rust core built on `kube-rs`, exposed to Swift through UniFFI. The driving constraint: minimal memory footprint and a panel that opens instantly, which no webview option (Tauri) delivers at the same time. In exchange we accept a hybrid `cargo` + Xcode build.

## Considered Options

- **Tauri v2** (ADR-0001): ~60–100 MB at rest with a live webview, or slow opening if destroyed.
- **Pure Rust with AppKit via `objc2`**: the leanest (~8–15 MB), but a hand-written AppKit UI in Rust, cascading form included, costs too much to write and maintain.
- **All Swift with `kubectl` as a subprocess**: no reliable pod switching, depends on `kubectl`; and reimplementing port-forward and `exec` auth in Swift would be heavy.

## Consequences

- The core does no polling: a single pod watch (filtered on the Target) per Active Forward, no watch for Inactive Forwards, `current_thread` tokio runtime.
- The UniFFI API is the boundary: Swift only knows Forwards, their Desired state and their Actual state, never `kube-rs`.
