// Modified for cpTerm: remove chrono; see vendor/README.md for patch details.
#[cfg(target_arch = "wasm32")]
compile_error!("cpTerm vendors russh-util for native platforms only");

pub mod runtime;
pub mod time;
