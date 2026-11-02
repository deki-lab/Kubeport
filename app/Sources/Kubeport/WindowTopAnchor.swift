import AppKit
import SwiftUI

/// Keeps the panel window glued to the menu bar when its content changes height.
///
/// `MenuBarExtra(.window)` resizes its window from the bottom-left corner (Cocoa
/// coordinates), so switching screens (list ↔ Settings ↔ form) moves the top edge away
/// from the menu bar. This view remembers the top edge when the panel opens and puts it
/// back after every resize.
struct WindowTopAnchor: NSViewRepresentable {
    func makeNSView(context: Context) -> AnchorView { AnchorView() }
    func updateNSView(_ nsView: AnchorView, context: Context) {}

    final class AnchorView: NSView {
        private var top: CGFloat?
        private var observers: [NSObjectProtocol] = []

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            observers.forEach(NotificationCenter.default.removeObserver)
            observers = []
            guard let window else { return }
            let center = NotificationCenter.default
            // Opening the panel: its position is right under the menu bar.
            observers.append(center.addObserver(forName: NSWindow.didBecomeKeyNotification, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.top = window.frame.maxY }
            })
            observers.append(center.addObserver(forName: NSWindow.didResizeNotification, object: window, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let top = self?.top, abs(window.frame.maxY - top) > 0.5 else { return }
                    window.setFrameOrigin(NSPoint(x: window.frame.minX, y: top - window.frame.height))
                }
            })
            if window.isKeyWindow { top = window.frame.maxY }
        }

        deinit { observers.forEach(NotificationCenter.default.removeObserver) }
    }
}
