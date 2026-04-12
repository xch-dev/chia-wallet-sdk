use bindy::Result;

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
