#[cfg(not(target_arch = "wasm32"))]
// #[path = "native.rs"]
mod native;

// #[cfg(target_arch = "wasm32")]
// #[path = "wasm.rs"]
mod wasm;
