import AppKit
import SwiftUI

/// Keeps the panel window glued to the menu bar and sized to its content.
///
/// `MenuBarExtra(.window)` grows its window when the content grows but does not shrink
/// it when the content gets shorter (back from Settings): the content then floats in a
/// window that is too tall. This view resizes the window to the measured content height,
/// keeping the top edge where the panel opened, right under the menu bar.
struct WindowTopAnchor: NSViewRepresentable {
    let contentHeight: CGFloat

    func makeNSView(context: Context) -> AnchorView { AnchorView() }
    func updateNSView(_ nsView: AnchorView, context: Context) { nsView.fit(height: contentHeight) }

    final class AnchorView: NSView {
        private var top: CGFloat?
        private var observers: [NSObjectProtocol] = []

        /// Resizes the window to `height`, keeping its top edge fixed.
        func fit(height: CGFloat) {
            guard let window, height > 1 else { return }
            let frame = window.frame
            guard abs(frame.height - height) > 0.5 else { return }
            let top = self.top ?? frame.maxY
            window.setFrame(NSRect(x: frame.minX, y: top - height, width: frame.width, height: height), display: true)
        }

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
