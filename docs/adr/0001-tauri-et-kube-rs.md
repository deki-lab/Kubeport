---
status: superseded by ADR-0003
---

# Tauri v2 + kube-rs, sans kubectl ni SwiftUI

L'app devait être écrite en Rust de bout en bout : Tauri v2 pour l'icône de barre de menu et un panneau HTML ancré sous l'icône, `kube-rs` pour parler directement à l'API Kubernetes. Abandonné avant toute implémentation quand la contrainte « mémoire minimale + panneau instantané » est apparue : la webview WebKit coûte ~60–100 Mo si on la garde en vie, et 200–500 ms à chaque ouverture si on la détruit.

## Considered Options

- **SwiftUI `MenuBarExtra` + `kubectl` en sous-processus** : rendu 100 % natif, mais dépendance à `kubectl` et supervision de processus fragile.
- **Rust pur (`tray-icon` + `muda`)** : vrai menu natif, mais un formulaire n'y tient pas : il faut une fenêtre egui/iced à l'aspect non natif.
- **SwiftUI + cœur Rust via UniFFI** : retenu ensuite, voir ADR-0003.
