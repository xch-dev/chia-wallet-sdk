use std::time::Duration;

use bindy::Result;

/// Converts a binding-facing `Option<u32>` milliseconds value into a `Duration`.
/// Used to bridge the bindings (which expose `u32` ms for FFI portability) to the
/// SDK layer (which takes `Duration`).
pub(crate) fn ms_to_duration(ms: Option<u32>) -> Option<Duration> {
    ms.map(|ms| Duration::from_millis(u64::from(ms)))
}

// UniFFI's C# async bridge polls Rust futures without a Tokio runtime context.
// napi-rs and pyo3-async-runtimes provide their own, so this is only needed for uniffi.
#[cfg(feature = "uniffi")]
static TOKIO_RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> =
    std::sync::LazyLock::new(|| {
        tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")
    });

/// Spawns a future on the Tokio runtime for uniffi, or awaits directly for other bindings.
#[cfg(feature = "uniffi")]
pub(crate) async fn spawn_on_runtime<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    TOKIO_RUNTIME
        .spawn(future)
        .await
        .map_err(|e| bindy::Error::Custom(format!("task join failed: {e}")))?
}

#[cfg(not(feature = "uniffi"))]
pub(crate) async fn spawn_on_runtime<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    future.await
}

/// Drives an async binding method to completion synchronously on the shared Tokio runtime.
///
/// Used by the `bindy_uniffi_sync!` macro (the C++ backend), since `uniffi-bindgen-cpp`
/// cannot generate async functions. Must be called from a foreign (non-async) thread; the
/// inner future spawns its work onto the same runtime via [`spawn_on_runtime`].
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
