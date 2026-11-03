# Contributing to Kubeport

## Project layout

```text
core/   Rust core: Session, Manager, Forward runtime, config; cluster/ is the seam to
        Kubernetes (kube-rs adapter, in-memory adapter for tests)
ffi/    UniFFI bridge exposing the core to Swift
cli/    Development CLI to drive the core without the UI
app/    SwiftUI menu bar app (Swift package) and its translations
docs/   Architecture decisions (adr/) and the UX spec (ux/)
```

Vocabulary: [GLOSSARY.md](GLOSSARY.md). Architecture decisions: [docs/adr/](docs/adr/).

## Build and test

Requirements: Rust (the version in `rust-toolchain.toml`, installed by rustup) and Xcode (for `xcodebuild -create-xcframework`).

```sh
cargo test --all                          # Rust core and FFI bridge
./scripts/build-app.sh                    # dist/Kubeport.app + dist/Kubeport-<version>.dmg (arm64 + x86_64)
swift test --package-path app             # app logic without SwiftUI (after build-app.sh)
python3 scripts/check-l10n.py             # translations: missing or unused keys, placeholders
git ls-files -z 'app/*.swift' | xargs -0 swift format -i   # Swift style (.swift-format); CI runs `lint --strict`
./scripts/measure-footprint.sh            # memory of the running app (budget: 35 MB)
```

`build-app.sh` runs `scripts/build-ffi.sh`, which builds the Rust core into `app/KubeportFFI.xcframework` and generates the Swift bindings. Run alone, `build-ffi.sh` builds for this Mac's architecture only: set `ARCHS="arm64 x86_64"` for both. Open `app/Package.swift` in Xcode to work on the UI.

PR checks run the Rust code and swift-format only (Linux). The app is built and tested on macOS (Apple silicon and Intel) only on the release PR, after a maintainer approves the `release-build` environment: when you change `app/`, `ffi/` or `scripts/`, run `./scripts/build-app.sh` and `swift test --package-path app` before opening the PR.

### Tests against a cluster

Some tests need a local cluster (Docker Desktop Kubernetes, context `docker-desktop`):

```sh
kubectl --context docker-desktop apply -f tests/fixtures/whoami.yaml
cargo test --all -- --ignored             # Idle timeout, form lists against the cluster
./scripts/rollout-test.sh                 # rollout under load: no failed request
```

### Running the app or the CLI during development

```sh
KUBEPORT_CONFIG=tests/fixtures/config.toml KUBEPORT_ACTIVATE=whoami \
  dist/Kubeport.app/Contents/MacOS/Kubeport

cargo run -p kubeport-cli -- --config tests/fixtures/config.toml list
cargo run -p kubeport-cli -- --config tests/fixtures/config.toml up whoami
```

## Conventions

- Everything in the repository is in English. The French UI lives in `app/Resources/fr.lproj`; every UI string goes through `L("key")` and must exist in both `en` and `fr` (checked by `check-l10n.py` on every build).
- Use the GLOSSARY terms in code, docs and UI copy.
- Pull requests are squash-merged; the PR title must follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat: …`, `fix: …`, `docs: …`). It becomes the commit and the changelog entry.
- Keep the app light: no polling, one watch per Active Forward, no web view.

## Releases

[release-please](https://github.com/googleapis/release-please) opens a release PR from the merged commits. Merging it tags the version, builds the `.dmg`, attaches it to the GitHub release and, when the `HOMEBREW_TAP_ENABLED` variable is `true`, updates the cask in [deki-lab/homebrew-tap](https://github.com/deki-lab/homebrew-tap) (secret `HOMEBREW_TAP_TOKEN`).
