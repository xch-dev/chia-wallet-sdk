use std::time::Duration;

/// Opt-in timeout configuration for the HTTP clients.
///
/// Both fields default to `None`, which preserves reqwest's default behavior of
/// no request timeout (the only implicit bound is the OS-level TCP connect timeout).
#[derive(Debug, Clone, Copy, Default)]
pub struct ClientOptions {
    /// Whole-request timeout, covering connect, send, and receiving the response.
    /// `None` leaves the request unbounded.
    pub timeout: Option<Duration>,
    /// Connection-phase timeout only. `None` falls back to the OS default.
    pub connect_timeout: Option<Duration>,
}

impl ClientOptions {
    /// Applies the configured timeouts to a reqwest [`ClientBuilder`](reqwest::ClientBuilder).
    ///
    /// On `wasm32` the reqwest fetch backend does not support builder-level timeouts
    /// (the browser controls them), so this is a no-op there.
    pub(crate) fn apply(self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut builder = builder;
            if let Some(timeout) = self.timeout {
                builder = builder.timeout(timeout);
            }
            if let Some(connect_timeout) = self.connect_timeout {
                builder = builder.connect_timeout(connect_timeout);
            }
            builder
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = (self.timeout, self.connect_timeout);
            builder
        }
    }
}
