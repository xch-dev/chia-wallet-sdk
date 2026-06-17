use std::time::Duration;

#[cfg(feature = "uniffi")]
use bindy::Result;

/// Converts a binding-facing `Option<u32>` milliseconds value into a `Duration`.
/// Used to bridge the bindings (which expose `u32` ms for FFI portability) to the
/// SDK layer (which takes `Duration`).
pub(crate) fn ms_to_duration(ms: Option<u32>) -> Option<Duration> {
    ms.map(|ms| Duration::from_millis(u64::from(ms)))
}

// Tokio runtime for the C++ sync backend's `block_on`
#[cfg(feature = "uniffi")]
static TOKIO_RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> =
    std::sync::LazyLock::new(|| {
        tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")
    });

/// Drives an async binding method to completion synchronously on a shared Tokio runtime.
///
/// Used by the `bindy_uniffi_sync!` macro (the C++ backend), since `uniffi-bindgen-cpp`
/// cannot generate async functions. Must be called from a foreign (non-async) thread.
///
/// There is intentionally no timeout here: per-operation timeouts are configured on the
/// clients themselves (`RpcClientOptions`, `PeerOptions`) so they apply uniformly across
/// every binding backend, not just this synchronous one.
#[cfg(feature = "uniffi")]
pub fn block_on<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    TOKIO_RUNTIME.block_on(future)
}
