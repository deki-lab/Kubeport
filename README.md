# Kubeport

Port-forwards Kubernetes depuis la barre de menu macOS. On configure un Forward une fois, on l'active d'un clic, et il suit automatiquement les pods derrière un Service (rollouts compris).

- Vocabulaire : [GLOSSARY.md](GLOSSARY.md)
- Décisions d'architecture : [docs/adr/](docs/adr/)

## Installation

```sh
brew install --cask deki-lab/tap/kubeport
```

L'app n'est pas encore signée par un Developer ID Apple ni notarisée. Au premier lancement, macOS la bloque :

1. Lancer Kubeport une première fois, puis fermer l'alerte.
2. Ouvrir **Réglages Système > Confidentialité et sécurité**.
3. En bas, cliquer **Ouvrir quand même** en face de Kubeport, puis confirmer.

Prérequis : macOS 14 ou plus récent, Mac Apple Silicon.

## Utilisation

- **Nouveau Forward** : Contexte, namespace, Service ou Pod et ports sont proposés depuis le cluster.
- Clic droit sur un Forward : **Modifier** (un Forward Actif est relancé) ou **Dupliquer**.
- **Réglages** (roue dentée) : lancement à l'ouverture de session, kubeconfig forcé.
- Les Forwards Actifs à la fermeture sont réactivés au lancement suivant.
- Fichiers, dans `~/Library/Application Support/k8s_forwarder/` :
  - `config.toml` : les Forwards, éditable à la main (bouton **Recharger** ensuite). L'app réécrit le fichier quand on l'édite depuis l'interface : les commentaires ajoutés à la main sont perdus.
  - `state.toml` : Forwards Actifs, géré par l'app.

## Développement

Prérequis : Rust stable, Xcode (pour `xcodebuild -create-xcframework`).

```sh
cargo test --all                 # cœur Rust et pont FFI
cargo test -p kubeport-ffi -- --ignored   # formulaire sur docker-desktop
cargo run -p kubeport-cli        # CLI de développement
./scripts/build-ffi.sh           # cœur Rust → app/KubeportFFI.xcframework + bindings Swift
./scripts/build-app.sh           # dist/Kubeport.app + dist/Kubeport-<version>-arm64.dmg
./scripts/measure-footprint.sh   # empreinte mémoire de l'app lancée (budget : 35 Mo)
./scripts/rollout-test.sh        # bout en bout sur docker-desktop : rollout sans coupure
```

Variables utiles pour lancer l'app en développement :

```sh
KUBEPORT_CONFIG=tests/fixtures/config.toml KUBEPORT_ACTIVATE=whoami \
  dist/Kubeport.app/Contents/MacOS/Kubeport
```

## CLI de développement

```sh
cargo run -p kubeport-cli -- --config tests/fixtures/config.toml list
cargo run -p kubeport-cli -- --config tests/fixtures/config.toml up whoami
```

Format de la config : voir `core/src/config.rs`.

## Releases

Les versions sont gérées par [release-please](https://github.com/googleapis/release-please) : merge en squash uniquement, titres de PR au format [Conventional Commits](https://www.conventionalcommits.org/) (`feat: …`, `fix: …`). Merger la PR de release crée le tag, construit le `.dmg`, l'attache à la release, et met à jour le cask du tap quand `HOMEBREW_TAP_ENABLED` vaut `true`.
