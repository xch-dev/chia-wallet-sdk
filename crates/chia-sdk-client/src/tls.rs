use std::fs;

use chia_ssl::ChiaCertificate;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use tokio_tungstenite::Connector;

use crate::ClientError;

/// Loads an SSL certificate, or creates it if it doesn't exist already.
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

/// Creates a native-tls connector from a certificate.
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

/// Creates a rustls connector from a certificate.
#[cfg(feature = "rustls")]
pub fn create_rustls_connector(cert: &ChiaCertificate) -> Result<Connector, ClientError> {
    use std::sync::Arc;

    use chia_ssl::CHIA_CA_CRT;
    use rustls::{
        client::danger::HandshakeSignatureValid,
        crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider},
        pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
        ClientConfig, DigitallySignedStruct, RootCertStore,
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
        rustls_pemfile::certs(&mut CHIA_CA_CRT.as_bytes()).collect::<Result<_, _>>()?;

    root_cert_store.add(ca.into_iter().next().ok_or(ClientError::MissingCa)?)?;

    let cert_chain: Vec<CertificateDer<'_>> =
        rustls_pemfile::certs(&mut cert.cert_pem.as_bytes()).collect::<Result<_, _>>()?;

    let key = rustls_pemfile::pkcs8_private_keys(&mut cert.key_pem.as_bytes())
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
