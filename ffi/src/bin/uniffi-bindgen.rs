//! Génère les bindings Swift : voir `scripts/build-ffi.sh`.

fn main() {
    uniffi::uniffi_bindgen_swift();
}
