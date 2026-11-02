# Interface SwiftUI, cœur Rust via UniFFI

L'interface est une app SwiftUI `MenuBarExtra` ; toute la logique Kubernetes (kubeconfig, auth `exec`, port-forward, résolution et bascule du Pod résolu, config TOML) vit dans un cœur Rust basé sur `kube-rs`, exposé à Swift via UniFFI. Contrainte qui l'impose : empreinte mémoire minimale et panneau qui s'ouvre instantanément, ce qu'aucune option à webview (Tauri) ne tient à la fois. On accepte en échange un build hybride `cargo` + Xcode.

## Considered Options

- **Tauri v2** (ADR-0001) : ~60–100 Mo au repos avec webview vivante, ou ouverture lente si détruite.
- **Rust pur avec AppKit via `objc2`** : le plus sobre (~8–15 Mo), mais une UI AppKit écrite à la main en Rust, formulaire en cascade compris, coûte trop cher à écrire et maintenir.
- **Tout en Swift avec `kubectl` en sous-processus** : pas de bascule de pod fiable, dépendance à `kubectl` ; et réimplémenter le port-forward et l'auth `exec` en Swift serait lourd.

## Consequences

- Le cœur ne fait aucun polling : un seul watch d'EndpointSlices par Forward Actif, aucun watch pour les Forwards Inactifs, runtime tokio `current_thread`.
- L'API exposée par UniFFI est la frontière : Swift ne connaît que les Forwards, leur État voulu et leur État réel, jamais `kube-rs`.
