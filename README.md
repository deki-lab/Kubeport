# Kubeport

Kubernetes port-forwards from the macOS menu bar. Configure a Forward once, turn it on with one click, and it automatically follows the pods behind a Service (rollouts included).

- Vocabulary: [GLOSSARY.md](GLOSSARY.md)
- Architecture decisions: [docs/adr/](docs/adr/)

## Installation

```sh
brew install --cask deki-lab/tap/kubeport
```

The app is not yet signed with an Apple Developer ID nor notarized. macOS blocks it on first launch:

1. Launch Kubeport once, then dismiss the alert.
2. Open **System Settings > Privacy & Security**.
3. At the bottom, click **Open Anyway** next to Kubeport, then confirm.

Requirements: macOS 14 or later, Apple Silicon Mac.

## Usage

The UI follows the macOS language: English or French (English for any other language).

- **New Forward**: Context, namespace, Service or Pod, and ports are suggested from the cluster.
- Right-click a Forward: **Edit** (an Active Forward is restarted) or **Duplicate**.
- **Settings** (gear icon): open at login, forced kubeconfig.
- Forwards that are Active when the app quits are reactivated on next launch.
- **Auto-deactivate**: an Active Forward with no open connection for the Idle timeout (in Settings, or per Forward: never, 15 min, 1 h, 4 h) turns Inactive on its own. In `config.toml`: `idle_timeout = 60` (minutes) at the top of the file or inside a `[[forward]]`, `"never"` to disable.
- Files, in `~/Library/Application Support/k8s_forwarder/`:
  - `config.toml`: the Forwards, hand-editable (then click **Reload**). The app rewrites the file when you edit from the UI: hand-written comments are lost.
  - `state.toml`: Active Forwards, managed by the app.

## Development

Requirements: stable Rust, Xcode (for `xcodebuild -create-xcframework`).

```sh
cargo test --all                 # Rust core and FFI bridge
cargo test -p kubeport-ffi -- --ignored   # form against docker-desktop
cargo run -p kubeport-cli        # development CLI
./scripts/build-ffi.sh           # Rust core → app/KubeportFFI.xcframework + Swift bindings
./scripts/build-app.sh           # dist/Kubeport.app + dist/Kubeport-<version>-arm64.dmg
./scripts/measure-footprint.sh   # memory footprint of the running app (budget: 35 MB)
./scripts/rollout-test.sh        # end to end on docker-desktop: rollout without downtime
python3 scripts/check-l10n.py      # translations: missing keys, unused keys, placeholders
```

Useful variables to run the app during development:

```sh
KUBEPORT_CONFIG=tests/fixtures/config.toml KUBEPORT_ACTIVATE=whoami \
  dist/Kubeport.app/Contents/MacOS/Kubeport
```

## Development CLI

```sh
cargo run -p kubeport-cli -- --config tests/fixtures/config.toml list
cargo run -p kubeport-cli -- --config tests/fixtures/config.toml up whoami
```

Config format: see `core/src/config.rs`.

## Releases

Versions are managed by [release-please](https://github.com/googleapis/release-please): squash merge only, PR titles in [Conventional Commits](https://www.conventionalcommits.org/) format (`feat: …`, `fix: …`). Merging the release PR creates the tag, builds the `.dmg`, attaches it to the release, and updates the tap's cask when `HOMEBREW_TAP_ENABLED` is `true`.
