pub struct RbServerConfig {
    pub host: String,
    pub port: u16,
    pub verbose: bool,
    pub mtls: MtlsConfig,
}

pub struct MtlsConfig {
    pub enabled: bool,
    pub ca_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,
    pub crl_path: String,
    pub crl_update_seconds: u64,
}

impl MtlsConfig {
    pub fn new(
        enabled: bool,
        ca_path: String,
        client_cert_path: String,
        client_key_path: String,
        crl_path: String,
        crl_update_seconds: u64,
    ) -> Self {
        MtlsConfig {
            enabled,
            ca_path,
            client_cert_path,
            client_key_path,
            crl_path,
            crl_update_seconds,
        }
    }

    pub fn default() -> Self {
        MtlsConfig {
            enabled: false,
            ca_path: "ca-cert.pem".to_string(),
            client_cert_path: "client-cert.pem".to_string(),
            client_key_path: "client-key.pem".to_string(),
            crl_path: "crl.der".to_string(),
            crl_update_seconds: 5,
        }
    }
}

impl RbServerConfig {
    pub fn new(host: String, port: u16, verbose: bool) -> RbServerConfig {
        RbServerConfig {
            host,
            port,
            verbose,
            mtls: MtlsConfig::default(),
        }
    }

    pub fn with_mtls(
        host: String,
        port: u16,
        verbose: bool,
        mtls_config: MtlsConfig,
    ) -> RbServerConfig {
        RbServerConfig {
            host,
            port,
            verbose,
            mtls: mtls_config,
        }
    }
}
