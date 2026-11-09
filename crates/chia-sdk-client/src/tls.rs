use std::{fs, io};

use chia_ssl::ChiaCertificate;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use tokio_tungstenite::Connector;

use crate::ClientError;

/// Loads an SSL certificate, or creates it if neither the certificate nor the key exist yet.
///
/// Any other error, including only one of the two files existing, is returned rather than
/// overwriting what's already on disk. The key is written so that only the owner can read it.
pub fn load_ssl_cert(cert_path: &str, key_path: &str) -> Result<ChiaCertificate, ClientError> {
    match (read_if_exists(cert_path)?, read_if_exists(key_path)?) {
        (Some(cert_pem), Some(key_pem)) => Ok(ChiaCertificate { cert_pem, key_pem }),
        (None, None) => {
            let cert = ChiaCertificate::generate()?;
            write_private(key_path, &cert.key_pem)?;
            fs::write(cert_path, &cert.cert_pem)?;
            Ok(cert)
        }
        (Some(_), None) => Err(not_found(key_path)),
        (None, Some(_)) => Err(not_found(cert_path)),
    }
}

fn read_if_exists(path: &str) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn not_found(path: &str) -> ClientError {
    io::Error::new(io::ErrorKind::NotFound, format!("{path} does not exist")).into()
}

fn write_private(path: &str, contents: &str) -> io::Result<()> {
    use std::io::Write;

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

    options.open(path)?.write_all(contents.as_bytes())
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

    use rustls::{
        ClientConfig, DigitallySignedStruct,
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

    let cert_chain: Vec<CertificateDer<'_>> =
        CertificateDer::pem_slice_iter(cert.cert_pem.as_bytes()).collect::<Result<_, _>>()?;

    let key = PrivatePkcs8KeyDer::pem_slice_iter(cert.key_pem.as_bytes())
        .next()
        .ok_or(ClientError::MissingPkcs8Key)??;

    // Peers use self-signed certificates, so the server certificate isn't verified.
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification(
            rustls::crypto::aws_lc_rs::default_provider(),
        )))
        .with_client_auth_cert(cert_chain, PrivateKeyDer::Pkcs8(key))?;

    Ok(Connector::Rustls(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn temp_dir(name: &str) -> io::Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!(
            "chia-sdk-client-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn path_str(path: &std::path::Path) -> &str {
        path.to_str().expect("temp path is not utf-8")
    }

    #[test]
    fn test_load_ssl_cert_generates_and_reloads() -> anyhow::Result<()> {
        let dir = temp_dir("generate")?;
        let cert_path = dir.join("cert.crt");
        let key_path = dir.join("cert.key");

        let generated = load_ssl_cert(path_str(&cert_path), path_str(&key_path))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&key_path)?.permissions().mode() & 0o777, 0o600);
        }

        let loaded = load_ssl_cert(path_str(&cert_path), path_str(&key_path))?;
        assert_eq!(generated.cert_pem, loaded.cert_pem);
        assert_eq!(generated.key_pem, loaded.key_pem);

        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn test_load_ssl_cert_does_not_overwrite_on_error() -> anyhow::Result<()> {
        let dir = temp_dir("no-overwrite")?;
        let cert_path = dir.join("cert.crt");
        let key_path = dir.join("cert.key");

        // The key is missing, so the existing certificate must be left alone.
        fs::write(&cert_path, "existing cert")?;
        assert!(load_ssl_cert(path_str(&cert_path), path_str(&key_path)).is_err());
        assert_eq!(fs::read_to_string(&cert_path)?, "existing cert");
        assert!(!key_path.exists());

        // The key can't be read (it's a directory), which isn't a reason to regenerate either.
        fs::create_dir(&key_path)?;
        assert!(load_ssl_cert(path_str(&cert_path), path_str(&key_path)).is_err());
        assert_eq!(fs::read_to_string(&cert_path)?, "existing cert");

        fs::remove_dir_all(dir)?;
        Ok(())
    }

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
