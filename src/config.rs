use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GatewayConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub grpc: GrpcConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default)]
    pub openapi: OpenApiConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenApiConfig {
    #[serde(default = "default_openapi_enabled")]
    pub enabled: bool,
    #[serde(default = "default_openapi_title")]
    pub title: String,
    #[serde(default = "default_openapi_version")]
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

#[derive(Debug, Deserialize)]
pub struct GrpcConfig {
    #[serde(default = "default_grpc_address")]
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_origins")]
    pub allowed_origins: Vec<String>,
}

fn default_port() -> u16 {
    8080
}
fn default_host() -> String {
    "::".to_string()
}
fn default_grpc_address() -> String {
    "http://localhost:50051".to_string()
}
fn default_origins() -> Vec<String> {
    vec!["*".to_string()]
}
fn default_openapi_enabled() -> bool {
    true
}
fn default_openapi_title() -> String {
    "Crap CMS REST API".to_string()
}
fn default_openapi_version() -> String {
    "1.0.0".to_string()
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self {
            enabled: default_openapi_enabled(),
            title: default_openapi_title(),
            version: default_openapi_version(),
        }
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            grpc: GrpcConfig::default(),
            cors: CorsConfig::default(),
            openapi: OpenApiConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            address: default_grpc_address(),
        }
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_origins(),
        }
    }
}

impl GatewayConfig {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: GatewayConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
