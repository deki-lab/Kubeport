# Changelog

## [0.4.0](https://github.com/deki-lab/Kubeport/compare/v0.3.0...v0.4.0) (2026-11-03)


### Features

* close a Forward after an hour without connection on new installs ([#42](https://github.com/deki-lab/Kubeport/issues/42)) ([e06c74a](https://github.com/deki-lab/Kubeport/commit/e06c74a90d309f14974b845ab6eb6248239da60c))
* do not restore a Forward edited while Kubeport was closed ([#43](https://github.com/deki-lab/Kubeport/issues/43)) ([a0930f0](https://github.com/deki-lab/Kubeport/commit/a0930f065bf9aac5a69d4f92422a912551e3ff3a))
* move the config folder to Application Support/Kubeport ([#44](https://github.com/deki-lab/Kubeport/issues/44)) ([f158286](https://github.com/deki-lab/Kubeport/commit/f158286efecec74b5cd3b2ff19c33735e0690f76))
* show the app's machine and pod in the Forward chips ([#40](https://github.com/deki-lab/Kubeport/issues/40)) ([15866c5](https://github.com/deki-lab/Kubeport/commit/15866c578847de2bd4e37a12800a8beeec36780a))

## [0.3.0](https://github.com/deki-lab/Kubeport/compare/v0.2.0...v0.3.0) (2026-11-03)


### Features

* app icon ([#21](https://github.com/deki-lab/Kubeport/issues/21)) ([2300c61](https://github.com/deki-lab/Kubeport/commit/2300c61428a7d9d245645daea86a74b0f265fbd4))
* build a universal app for Apple silicon and Intel Macs ([#26](https://github.com/deki-lab/Kubeport/issues/26)) ([d9cda8f](https://github.com/deki-lab/Kubeport/commit/d9cda8f93358a45982b5d8b767cde932af6bccbf))
* draw the app icon's machine and pod in the menu bar ([#29](https://github.com/deki-lab/Kubeport/issues/29)) ([e551229](https://github.com/deki-lab/Kubeport/commit/e551229d654a6daa88f70b75b775a7790ebc0e57))


### Bug Fixes

* apply the security audit and the last architecture recommendations ([#24](https://github.com/deki-lab/Kubeport/issues/24)) ([85b6ed9](https://github.com/deki-lab/Kubeport/commit/85b6ed93b052b02afdbe5b463c0387d7e5e5bac0))
* serve a Forward's local port only to the user running Kubeport ([#25](https://github.com/deki-lab/Kubeport/issues/25)) ([6f726f2](https://github.com/deki-lab/Kubeport/commit/6f726f2af41a9bf3fb26b999329f9bf8dad5b754))

## [0.2.0](https://github.com/deki-lab/k8s_forwarder/compare/v0.1.0...v0.2.0) (2026-11-02)


### Features

* auto-deactivate Forwards after an idle timeout ([2d8532b](https://github.com/deki-lab/k8s_forwarder/commit/2d8532b034cc37442ed6b26d1236738d28617eef))
* English and French UI following the macOS language ([5a181ff](https://github.com/deki-lab/k8s_forwarder/commit/5a181ffb6a2b09fd8c36a94b9e894745ba2374ad))
* Wi-Fi menu style panel ([#16](https://github.com/deki-lab/k8s_forwarder/issues/16)) ([3137932](https://github.com/deki-lab/k8s_forwarder/commit/313793209b449fc251ce96e50dd8abecadd11a04))

## 0.1.0 (2026-11-02)


### Features

* SwiftUI menu bar wired to the Rust core (slice 2) ([#5](https://github.com/deki-lab/k8s_forwarder/issues/5)) ([cf32e32](https://github.com/deki-lab/k8s_forwarder/commit/cf32e32f6dbbd724ef1bf38920c98a6fcd5cce08))
* Rust core and development CLI (slice 1) ([#4](https://github.com/deki-lab/k8s_forwarder/issues/4)) ([3ec3c53](https://github.com/deki-lab/k8s_forwarder/commit/3ec3c53414e5d54dd08c272c390af40b5621041f))
* create and edit Forwards from the menu bar (slice 3) ([#7](https://github.com/deki-lab/k8s_forwarder/issues/7)) ([bba6595](https://github.com/deki-lab/k8s_forwarder/commit/bba65954df51339f479207b3ef1a2d1b6a9ad7c7))
