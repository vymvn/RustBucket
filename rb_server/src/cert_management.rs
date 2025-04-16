use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use rustls::{Certificate, PrivateKey, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};
use rcgen::{CertificateParams, DnType, CertifiedKey, KeyPair, RcgenError};

/// Struct to handle certificate management for the C2 server
pub struct CertManager {
    ca_cert: Option<CertifiedKey>,
    server_cert: Option<CertifiedKey>,
    client_cert: Option<CertifiedKey>,
    cert_dir: PathBuf,
}

impl CertManager {
    /// Create a new certificate manager
    pub fn new(cert_dir: impl AsRef<Path>) -> Self {
        let cert_path = PathBuf::from(cert_dir.as_ref());
        
        // Create directories if they don't exist
        std::fs::create_dir_all(&cert_path).unwrap_or_else(|e| {
            log::warn!("Failed to create certificate directory: {}", e);
        });
        
        CertManager {
            ca_cert: None,
            server_cert: None,
            client_cert: None,
            cert_dir: cert_path,
        }
    }
    
    /// Generate all certificates needed for mTLS
    pub fn generate_certificates(&mut self) -> Result<(), RcgenError> {
        // Generate CA certificate
        let alg = &rcgen::PKCS_ECDSA_P256_SHA256;
        let mut ca_params = CertificateParams::new(Vec::new())?;
        ca_params.distinguished_name.push(DnType::OrganizationName, "RustBucket C2");
        ca_params.distinguished_name.push(DnType::CommonName, "RustBucket CA");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        
        let ca_cert = rcgen::Certificate::from_params(ca_params)?;
        
        // Generate server certificate
        let mut server_params = CertificateParams::new(vec!["localhost".to_string()])?;
        server_params.distinguished_name.push(DnType::OrganizationName, "RustBucket C2");
        server_params.distinguished_name.push(DnType::CommonName, "RustBucket Server");
        let server_cert = rcgen::Certificate::from_params(server_params)?;
        let server_cert_signed = server_cert.serialize_pem_with_signer(&ca_cert)?;
        
        // Generate client certificate template
        let mut client_params = CertificateParams::new(Vec::new())?;
        client_params.distinguished_name.push(DnType::OrganizationName, "RustBucket C2");
        client_params.distinguished_name.push(DnType::CommonName, "RustBucket Client");
        let client_cert = rcgen::Certificate::from_params(client_params)?;
        let client_cert_signed = client_cert.serialize_pem_with_signer(&ca_cert)?;
        
        // Store the generated certificates
        self.ca_cert = Some(CertifiedKey {
            cert: ca_cert.serialize_pem()?.as_bytes().to_vec(),
            key_pair: ca_cert.get_key_pair().clone(),
        });
        
        self.server_cert = Some(CertifiedKey {
            cert: server_cert_signed.as_bytes().to_vec(),
            key_pair: server_cert.get_key_pair().clone(),
        });
        
        self.client_cert = Some(CertifiedKey {
            cert: client_cert_signed.as_bytes().to_vec(),
            key_pair: client_cert.get_key_pair().clone(),
        });
        
        // Save certificates to disk
        self.save_certificates()?;
        
        Ok(())
    }
    
    /// Save all certificates to the filesystem
    fn save_certificates(&self) -> Result<(), RcgenError> {
        if let Some(ca_cert) = &self.ca_cert {
            let ca_cert_path = self.cert_dir.join("ca-cert.pem");
            let ca_key_path = self.cert_dir.join("ca-key.pem");
            
            let mut ca_cert_file = File::create(ca_cert_path).unwrap();
            ca_cert_file.write_all(&ca_cert.cert).unwrap();
            
            let ca_key = ca_cert.key_pair.serialize_pem();
            let mut ca_key_file = File::create(ca_key_path).unwrap();
            ca_key_file.write_all(ca_key.as_bytes()).unwrap();
        }
        
        if let Some(server_cert) = &self.server_cert {
            let server_cert_path = self.cert_dir.join("server-cert.pem");
            let server_key_path = self.cert_dir.join("server-key.pem");
            
            let mut server_cert_file = File::create(server_cert_path).unwrap();
            server_cert_file.write_all(&server_cert.cert).unwrap();
            
            let server_key = server_cert.key_pair.serialize_pem();
            let mut server_key_file = File::create(server_key_path).unwrap();
            server_key_file.write_all(server_key.as_bytes()).unwrap();
        }
        
        if let Some(client_cert) = &self.client_cert {
            let client_cert_path = self.cert_dir.join("client-cert.pem");
            let client_key_path = self.cert_dir.join("client-key.pem");
            
            let mut client_cert_file = File::create(client_cert_path).unwrap();
            client_cert_file.write_all(&client_cert.cert).unwrap();
            
            let client_key = client_cert.key_pair.serialize_pem();
            let mut client_key_file = File::create(client_key_path).unwrap();
            client_key_file.write_all(client_key.as_bytes()).unwrap();
        }
        
        Ok(())
    }
    
    /// Create a rustls ServerConfig from the generated certificates
    pub fn create_server_config(&self) -> Result<Arc<ServerConfig>, std::io::Error> {
        let server_cert_path = self.cert_dir.join("server-cert.pem");
        let server_key_path = self.cert_dir.join("server-key.pem");
        let ca_cert_path = self.cert_dir.join("ca-cert.pem");
        
        // Load CA certificate for client verification
        let mut ca_file = File::open(ca_cert_path)?;
        let ca_certs = certs(&mut ca_file)?;
        let ca_certs = ca_certs.into_iter().map(Certificate).collect::<Vec<_>>();
        
        // Load server certificate and key
        let mut cert_file = File::open(server_cert_path)?;
        let cert_chain = certs(&mut cert_file)?;
        let cert_chain = cert_chain.into_iter().map(Certificate).collect::<Vec<_>>();
        
        let mut key_file = File::open(server_key_path)?;
        let mut keys = pkcs8_private_keys(&mut key_file)?;
        if keys.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No keys found in key file",
            ));
        }
        
        // Build client verification from CA
        let client_auth = rustls::server::AllowAnyAuthenticatedClient::new(
            rustls::RootCertStore::from_iter(ca_certs.iter().cloned())
        );
        
        // Build server config
        let server_config = ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(client_auth)
            .with_single_cert(cert_chain, PrivateKey(keys.remove(0)))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        
        Ok(Arc::new(server_config))
    }
    
    /// Get paths to client certificates for distribution
    pub fn get_client_cert_paths(&self) -> (PathBuf, PathBuf, PathBuf) {
        (
            self.cert_dir.join("ca-cert.pem"),
            self.cert_dir.join("client-cert.pem"),
            self.cert_dir.join("client-key.pem")
        )
    }
}
