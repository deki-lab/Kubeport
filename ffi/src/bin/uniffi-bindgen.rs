//! Generates the Swift bindings: see `scripts/build-ffi.sh`.

fn main() {
    uniffi::uniffi_bindgen_swift();
}
