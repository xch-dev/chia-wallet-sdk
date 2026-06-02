use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bindy::Result;

// Backstop timeout (in milliseconds) for the synchronous `block_on` bridge.
// `0` means disabled, which preserves the original "block until the future resolves"
// behavior. Only consulted by `block_on` (the uniffi/C++ sync path); the async
// backends rely on their host runtime to impose timeouts.
static BLOCK_ON_TIMEOUT_MS: AtomicU64 = AtomicU64::new(0);

/// Sets a process-wide backstop timeout for the synchronous binding bridge.
///
/// This is the internal setter behind the public `set_blocking_call_timeout` binding.
/// Passing `None` (or a zero duration) disables it, restoring the default behavior
/// of blocking until the future completes. Affects only the synchronous (C++)
/// backend, which drives futures to completion via [`block_on`].
pub fn set_block_on_timeout(timeout: Option<Duration>) {
    let millis = timeout.map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    BLOCK_ON_TIMEOUT_MS.store(millis, Ordering::Relaxed);
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
        .map_err(|e| bindy::Error::Custom(e.to_string()))?
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
#[cfg(feature = "uniffi")]
pub fn block_on<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let millis = BLOCK_ON_TIMEOUT_MS.load(Ordering::Relaxed);

    if millis == 0 {
        return TOKIO_RUNTIME.block_on(future);
    }

    TOKIO_RUNTIME.block_on(async move {
        match tokio::time::timeout(Duration::from_millis(millis), future).await {
            Ok(result) => result,
            Err(_elapsed) => Err(bindy::Error::Custom(format!(
                "operation timed out after {millis}ms"
            ))),
        }
    })
}
