import AppKit

/// Icône de la barre de menu : contour (aucun Forward Actif), pleine (au moins un Actif),
/// pleine avec un point rouge (au moins un Forward en Erreur).
enum StatusIcon {
    private static let idle = template("arrow.left.arrow.right.circle")
    private static let active = template("arrow.left.arrow.right.circle.fill")
    private static let error = withRedDot(active)

    static func image(for state: GlobalState) -> NSImage {
        switch state {
        case .idle: idle
        case .active: active
        case .error: error
        }
    }

    private static func template(_ symbol: String) -> NSImage {
        let config = NSImage.SymbolConfiguration(pointSize: 15, weight: .regular)
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: "Kubeport")!
            .withSymbolConfiguration(config)!
        image.isTemplate = true
        return image
    }

    /// Le point rouge interdit le mode template : on dessine le symbole dans la couleur
    /// du texte de la barre de menu au moment du rendu (clair ou sombre).
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
        image.accessibilityDescription = "Kubeport : erreur"
        return image
    }
}
