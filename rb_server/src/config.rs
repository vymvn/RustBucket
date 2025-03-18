pub struct RbServerConfig {
    pub host: String,
    pub port: u16,
    pub verbose: bool,
}

impl RbServerConfig {
    pub fn new(host: String, port: u16, verbose: bool) -> RbServerConfig {
        RbServerConfig { host, port, verbose }
    }
}
