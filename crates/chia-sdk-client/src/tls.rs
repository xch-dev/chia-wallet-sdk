use std::fs;

use chia_ssl::ChiaCertificate;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use tokio_tungstenite::Connector;

use crate::ClientError;

/// Loads a TLS certificate and private key from PEM files, or generates new ones if they
/// can't be read.
///
/// If either file can't be read, a new certificate signed by the Chia CA is generated with
/// [`ChiaCertificate::generate`] and written to both paths, replacing any existing file.
/// The contents of existing files aren't validated.
///
/// The certificate is used to build a [`Connector`](tokio_tungstenite::Connector) with
/// `create_native_tls_connector` or `create_rustls_connector`.
///
/// # Example
///
/// ```
/// use chia_sdk_client::load_ssl_cert;
///
/// let dir = std::env::temp_dir().join(format!("chia-sdk-client-doctest-{}", std::process::id()));
/// std::fs::create_dir_all(&dir)?;
/// let cert_path = dir.join("wallet.crt");
/// let key_path = dir.join("wallet.key");
/// let (cert_path, key_path) = (cert_path.to_str().unwrap(), key_path.to_str().unwrap());
///
/// // The first call generates a certificate, and later calls load the same one.
/// let generated = load_ssl_cert(cert_path, key_path)?;
/// let loaded = load_ssl_cert(cert_path, key_path)?;
/// assert_eq!(generated.cert_pem, loaded.cert_pem);
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_ssl_cert(cert_path: &str, key_path: &str) -> Result<ChiaCertificate, ClientError> {
    fs::read_to_string(cert_path)
        .and_then(|cert| {
            fs::read_to_string(key_path).map(|key| ChiaCertificate {
                cert_pem: cert,
                key_pem: key,
            })
        })
        .or_else(|_| {
            let cert = ChiaCertificate::generate()?;
            fs::write(cert_path, &cert.cert_pem)?;
            fs::write(key_path, &cert.key_pem)?;
            Ok(cert)
        })
}

/// Creates a `native-tls` [`Connector`] that authenticates with the given client certificate.
///
/// The peer's certificate isn't verified, since full nodes use self-signed certificates.
///
/// # Example
///
/// ```
/// use chia_sdk_client::create_native_tls_connector;
/// use chia_ssl::ChiaCertificate;
///
/// let cert = ChiaCertificate::generate()?;
/// let connector = create_native_tls_connector(&cert)?;
/// # let _ = connector;
/// # Ok::<(), chia_sdk_client::ClientError>(())
/// ```
#[cfg(feature = "native-tls")]
pub fn create_native_tls_connector(cert: &ChiaCertificate) -> Result<Connector, ClientError> {
    use native_tls::{Identity, TlsConnector};

    let identity = Identity::from_pkcs8(cert.cert_pem.as_bytes(), cert.key_pem.as_bytes())?;
    let tls_connector = TlsConnector::builder()
        .identity(identity)
        .danger_accept_invalid_certs(true)
        .build()?;

    Ok(Connector::NativeTls(tls_connector))
}

/// Creates a `rustls` [`Connector`] that authenticates with the given client certificate.
///
/// The peer's certificate chain isn't verified, since full nodes use self-signed
/// certificates, but the handshake signatures are still checked.
///
/// # Example
///
/// ```
/// use chia_sdk_client::create_rustls_connector;
/// use chia_ssl::ChiaCertificate;
///
/// let cert = ChiaCertificate::generate()?;
/// let connector = create_rustls_connector(&cert)?;
/// # let _ = connector;
/// # Ok::<(), chia_sdk_client::ClientError>(())
/// ```
#[cfg(feature = "rustls")]
pub fn create_rustls_connector(cert: &ChiaCertificate) -> Result<Connector, ClientError> {
    use std::sync::Arc;

    use chia_ssl::CHIA_CA_CRT;
    use rustls::{
        ClientConfig, DigitallySignedStruct, RootCertStore,
        client::danger::HandshakeSignatureValid,
        crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
        pki_types::{
            CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime, pem::PemObject,
        },
    };

    #[derive(Debug)]
    struct NoCertificateVerification(CryptoProvider);

    impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls12_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            verify_tls13_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    let mut root_cert_store = RootCertStore::empty();

    let ca: Vec<CertificateDer<'_>> =
        CertificateDer::pem_slice_iter(CHIA_CA_CRT.as_bytes()).collect::<Result<_, _>>()?;

    root_cert_store.add(ca.into_iter().next().ok_or(ClientError::MissingCa)?)?;

    let cert_chain: Vec<CertificateDer<'_>> =
        CertificateDer::pem_slice_iter(cert.cert_pem.as_bytes()).collect::<Result<_, _>>()?;

    let key = PrivatePkcs8KeyDer::pem_slice_iter(cert.key_pem.as_bytes())
        .next()
        .ok_or(ClientError::MissingPkcs8Key)??;

    let mut config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_client_auth_cert(cert_chain, PrivateKeyDer::Pkcs8(key))?;

    config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertificateVerification(
            rustls::crypto::aws_lc_rs::default_provider(),
        )));

    Ok(Connector::Rustls(Arc::new(config)))
}

#[cfg(all(test, any(feature = "native-tls", feature = "rustls")))]
mod tests {
    use super::*;

    #[cfg(feature = "rustls")]
    #[test]
    fn test_create_rustls_connector() -> Result<(), ClientError> {
        let cert = ChiaCertificate::generate()?;
        create_rustls_connector(&cert)?;
        Ok(())
    }

    #[cfg(feature = "native-tls")]
    #[test]
    fn test_create_native_tls_connector() -> Result<(), ClientError> {
        let cert = ChiaCertificate::generate()?;
        create_native_tls_connector(&cert)?;
        Ok(())
    }
}
