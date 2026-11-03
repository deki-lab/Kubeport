import AppKit

/// Menu bar icon: the app icon's drawing, a machine (square) linked to a pod (hexagon).
/// Outline hexagon: no Active Forward. Filled hexagon: at least one Active.
/// Filled with a red dot: at least one Forward in Error.
enum StatusIcon {
    private static let idle = glyph(filled: false)
    private static let active = glyph(filled: true)
    private static let error = withRedDot(active)

    static func image(for state: GlobalState) -> NSImage {
        switch state {
        case .idle: idle
        case .active: active
        case .error: error
        }
    }

    private static let size = NSSize(width: 27, height: 16)
    private static let lineWidth: CGFloat = 1.5

    /// The drawing alone, as a template image `width` points wide: also used in the panel
    /// (Forward chips, empty list). Drawn at that size, so it stays sharp.
    static func glyph(filled: Bool, width: CGFloat = 27) -> NSImage {
        let scale = width / size.width
        let image = NSImage(size: NSSize(width: width, height: size.height * scale), flipped: false) { _ in
            let transform = NSAffineTransform()
            transform.scale(by: scale)
            transform.concat()
            draw(filled: filled)
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = L("icon.label")
        return image
    }

    /// Same layout as packaging/icon/AppIcon.svg, scaled to the menu bar.
    private static func draw(filled: Bool) {
        NSColor.black.set()
        let machine = NSBezierPath(
            roundedRect: NSRect(x: 1.75, y: 3.75, width: 8.5, height: 8.5), xRadius: 2, yRadius: 2)
        machine.lineWidth = lineWidth
        machine.stroke()

        let link = NSBezierPath()
        link.move(to: NSPoint(x: 10.25, y: 8))
        link.line(to: NSPoint(x: 14.5, y: 8))
        link.lineWidth = lineWidth
        link.stroke()

        // Pointy-top hexagon; the outline is inset so its stroke stays in the canvas.
        let center = NSPoint(x: 20.1, y: 8)
        let radius: CGFloat = filled ? 6.75 : 6
        let pod = NSBezierPath()
        for k in 0..<6 {
            let angle = (90 + 60 * CGFloat(k)) * .pi / 180
            let point = NSPoint(x: center.x + radius * cos(angle), y: center.y + radius * sin(angle))
            if k == 0 { pod.move(to: point) } else { pod.line(to: point) }
        }
        pod.close()
        if filled {
            pod.fill()
        } else {
            pod.lineWidth = lineWidth
            pod.lineJoinStyle = .round
            pod.stroke()
        }
    }

    /// The red dot rules out template mode: the drawing is tinted with the menu bar
    /// text color at render time (light or dark).
    private static func withRedDot(_ base: NSImage) -> NSImage {
        let size = base.size
        let image = NSImage(size: size, flipped: false) { rect in
            let tinted = NSImage(size: size, flipped: false) { r in
                base.draw(in: r)
                NSColor.labelColor.set()
                r.fill(using: .sourceAtop)
                return true
            }
            tinted.draw(in: rect)
            let dot = NSRect(x: rect.maxX - 6, y: rect.minY, width: 6, height: 6)
            NSColor.systemRed.setFill()
            NSBezierPath(ovalIn: dot).fill()
            return true
        }
        image.isTemplate = false
        image.accessibilityDescription = L("icon.labelError")
        return image
    }
}
