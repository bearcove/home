use camino::Utf8PathBuf;
use facet::Facet;
use serde::{Deserialize, Serialize};

plait::plait! {
    with crates {
        serde
    }

    /// A domain name/tenant name, like `fasterthanli.me` or `ftl.snug.blog`
    pub struct TenantDomain => &TenantDomainRef;

    /// An S3 endpoint, like `s3.us-east-1.amazonaws.com` or `nbg1.your-objectstorage.com`
    pub struct S3Endpoint => &S3EndpointRef;

    /// An S3 bucket name, like `bearcove-videos` or `ftl-revisions`
    pub struct S3BucketName => &S3BucketNameRef;

    /// An S3 region name, like `us-east-1` or `eu-central-1`
    pub struct S3RegionName => &S3RegionNameRef;

    /// An API key to access mom
    pub struct MomApiKey => &MomApiKeyRef;
}

/// The result of `load_cub_config`
#[derive(Facet)]
pub struct CubConfigBundle {
    pub cc: CubConfig,
    pub tenants: HashMap<TenantDomain, TenantInfo>,
}

impl TenantDomainRef {
    /// Return something that prints prettily in logs
    pub fn as_pretty(&self) -> PrettyTenantDomain {
        PrettyTenantDomain(self.to_owned())
    }
}

impl TenantDomain {
    /// Return something that prints prettily in logs
    pub fn into_pretty(self) -> PrettyTenantDomain {
        PrettyTenantDomain(self)
    }
}
#[derive(Facet)]
pub struct PrettyTenantDomain(TenantDomain);

impl std::fmt::Display for PrettyTenantDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\x1b[35m{}\x1b[0m", self.0)
    }
}

#[derive(Facet, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CubConfig {
    /// size the disk cache is allowed to use
    #[serde(skip_serializing)]
    #[serde(default = "serde_defaults::default_disk_cache_size")]
    pub disk_cache_size: ByteSize,

    /// Listen address without http, something like "127.0.0.1:1111"
    #[serde(default = "serde_defaults::cub_address")]
    pub address: SocketAddr,

    /// If the favorite port is taken, try to find a random port
    #[serde(default = "serde_defaults::random_port_fallback")]
    pub random_port_fallback: bool,

    /// Something like `http://localhost:1118`
    /// or `http://mom.svc.cluster.local:1118`, never
    /// a trailing slash.
    #[serde(default = "serde_defaults::mom_base_url")]
    pub mom_base_url: String,

    /// API key used to talk to mom
    #[serde(default = "serde_defaults::mom_api_key")]
    pub mom_api_key: MomApiKey,

    /// Where to store tenant data (think `/var/www/sites` or something)
    pub tenant_data_dir: Option<Utf8PathBuf>,

    /// Reddit-specific secrets
    pub reddit_secrets: Option<RedditSecrets>,
}

#[derive(Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomConfig {
    /// Tenant data dir
    pub tenant_data_dir: Utf8PathBuf,

    /// Mom-specific secrets
    pub secrets: MomSecrets,
}

/// Just enough information to build web/cdn URLs
#[derive(Facet, Debug, Copy, Clone)]
pub struct WebConfig {
    /// development or production
    pub env: Environment,

    /// the port we listen on
    pub port: u16,
}

impl CubConfig {
    /// Returns the webconfig from this config
    pub fn web_config(&self) -> WebConfig {
        WebConfig {
            env: Environment::default(),
            port: self.address.port(),
        }
    }
}

/// tenant-specific configuration that's common betweeen mom and cub
#[derive(Facet, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantConfig {
    /// tenant name (and domain name)
    #[serde(default = "serde_defaults::tenant_name")]
    pub name: TenantDomain,

    /// domain aliases for redirecting old domains to the current domain
    #[serde(default)]
    pub domain_aliases: Vec<TenantDomain>,

    /// used to access S3 bucket for assets etc.
    pub object_storage: Option<ObjectStorageConfig>,

    /// tenant-specific secrets (patreon/github oauth etc.)
    pub secrets: Option<TenantSecrets>,
}

impl TenantConfig {
    /// Empty config with just a name
    pub fn new(name: TenantDomain) -> Self {
        Self {
            name,
            domain_aliases: Default::default(),
            object_storage: None,
            secrets: None,
        }
    }

    /// Used to derive the secret key for cookie encryption
    pub fn cookie_sauce(&self) -> String {
        if let Some(secrets) = &self.secrets {
            if let Some(cookie_sauce) = &secrets.cookie_sauce {
                return cookie_sauce.clone();
            }
        }
        panic!("Cookie sauce not set for tenant {}! This should be derived by Mom from the global secret.", self.name);
    }

    /// e.g. for fasterthanli.me in prod, returns "fasterthanli.me".
    pub fn web_domain(&self, env: Environment) -> TenantDomain {
        match env {
            Environment::Development => TenantDomain::new(format!("{}.snug.blog", self.name)),
            Environment::Production => self.name.clone(),
        }
    }

    /// e.g. for fasterthanli.me in prod, returns "cdn.fasterthanli.me".
    pub fn cdn_domain(&self, env: Environment) -> TenantDomain {
        let base = match env {
            Environment::Development => format!("cdn.{}.snug.blog", self.name),
            Environment::Production => format!("cdn.{}", self.name),
        };
        TenantDomain::new(base)
    }
    /// Returns something like `https://fasterthanli.me` in prod or
    /// `http://fasterthanli.me.snug.blog:PORT` in dev
    pub fn web_base_url(&self, web_config: WebConfig) -> String {
        let name = &self.name;
        match web_config.env {
            Environment::Production => {
                format!("https://{name}")
            }
            Environment::Development => {
                let port = web_config.port;
                if let Ok(_var) = std::env::var("CUB_HTTPS") {
                    format!("https://{name}.snug.blog:{port}")
                } else {
                    format!("http://{name}.snug.blog:{port}")
                }
            }
        }
    }

    /// Returns something like `https://cdn.fasterthanli.me` in prod or
    /// `http://cdn.fasterthanli.me.snug.blog:PORT` in dev
    pub fn cdn_base_url(&self, web_config: WebConfig) -> String {
        let name = &self.name;
        match web_config.env {
            Environment::Production => {
                format!("https://cdn.{name}")
            }
            Environment::Development => {
                let port = web_config.port;
                if let Ok(_var) = std::env::var("CUB_HTTPS") {
                    format!("https://cdn.{name}.snug.blog:{port}")
                } else {
                    format!("http://cdn.{name}.snug.blog:{port}")
                }
            }
        }
    }

    pub fn secrets(&self) -> eyre::Result<&TenantSecrets> {
        if let Some(secrets) = &self.secrets {
            Ok(secrets)
        } else {
            eyre::bail!("Tenant secrets not specified for tenant {}", self.name)
        }
    }

    pub fn patreon_secrets(&self) -> eyre::Result<&PatreonSecrets> {
        self.secrets().and_then(|secrets| {
            if let Some(ref patreon) = secrets.patreon {
                Ok(patreon)
            } else {
                eyre::bail!("Patreon secrets not specified for tenant {}", self.name)
            }
        })
    }

    pub fn github_secrets(&self) -> eyre::Result<&GitHubSecrets> {
        self.secrets().and_then(|secrets| {
            if let Some(ref github) = secrets.github {
                Ok(github)
            } else {
                eyre::bail!("GitHub secrets not specified for tenant {}", self.name)
            }
        })
    }

    pub fn object_storage(&self) -> eyre::Result<&ObjectStorageConfig> {
        if let Some(object_storage) = &self.object_storage {
            Ok(object_storage)
        } else {
            eyre::bail!(
                "Object storage config not specified for tenant {}",
                self.name
            )
        }
    }
}

/// Info that cub has about a tenant.
#[derive(Facet, Clone)]
pub struct TenantInfo {
    /// Where the tenant's data is stored (assets, etc.)
    pub base_dir: Utf8PathBuf,

    /// Tenant config, received from mom
    pub tc: TenantConfig,
}

impl TenantInfo {
    pub fn internal_dir(&self) -> Utf8PathBuf {
        self.base_dir.join(".home")
    }

    pub fn vite_config_path(&self) -> Utf8PathBuf {
        self.internal_dir().join("vite.config.js")
    }

    pub fn content_dir(&self) -> Utf8PathBuf {
        self.base_dir.join("content")
    }

    pub fn templates_dir(&self) -> Utf8PathBuf {
        self.base_dir.join("templates")
    }

    pub fn mom_db_file(&self) -> Utf8PathBuf {
        self.internal_dir().join("mom.db")
    }
}

/// That config is part of the revision paks — it's stored in `home.config.json` and
/// contains no secrets at all
#[derive(Facet, Clone, Default, Serialize, Deserialize)]
#[facet(default)]
#[serde(deny_unknown_fields)]
pub struct RevisionConfig {
    /// where to push this site in prod?
    pub id: String,

    /// note: domains are configured on mom's side so folks can't
    /// randomly override, say, `fasterthanli.me`, with whatever they want.

    /// Patreon campaign IDs to allow access
    #[serde(default)]
    pub patreon_campaign_ids: Vec<String>,

    /// admin github user IDs
    #[serde(default)]
    pub admin_github_ids: Vec<String>,

    /// admin patreon user IDs
    #[serde(default)]
    pub admin_patreon_ids: Vec<String>,

    /// SVG font face collection
    #[serde(default)]
    pub svg_fonts: Vec<SvgFontSpec>,
}

#[derive(Facet, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SvgFontSpec {
    /// how the font is referred to in CSS, e.g. `IosevkaFtl`
    pub family: String,

    /// where to find the font on disk (relative to the base directory, ie. where `content` is)
    pub path: Utf8PathBuf,

    /// weight: 400 is normal, 700 is bold, etc.
    pub weight: FontWeight,

    /// style: normal, etc.
    pub style: FontStyle,
}

#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[facet(transparent)]
pub struct FontWeight(pub u16);

impl std::fmt::Display for FontWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for FontWeight {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let res = s.parse::<u16>();
        res.map(FontWeight)
    }
}

impl FontWeight {
    pub fn as_number(&self) -> u16 {
        self.0
    }

    pub fn as_css_prop(&self) -> String {
        format!("font-weight:{};", self.0)
    }
}

#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[facet(rename_all = "snake_case")]
#[repr(u8)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

impl std::fmt::Display for FontStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontStyle::Normal => write!(f, "normal"),
            FontStyle::Italic => write!(f, "italic"),
        }
    }
}

impl std::str::FromStr for FontStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal" => Ok(FontStyle::Normal),
            "italic" => Ok(FontStyle::Italic),
            _ => Err(format!("Unknown font style: {s}")),
        }
    }
}

impl FontStyle {
    pub fn as_css_prop(&self) -> &'static str {
        match self {
            FontStyle::Normal => "font-style:normal;",
            FontStyle::Italic => "font-style:italic;",
        }
    }
}

mod serde_defaults {
    use crate::{MOM_DEV_API_KEY, TenantDomain};

    pub(super) fn tenant_name() -> TenantDomain {
        "(unset)".into()
    }

    pub(super) fn cub_address() -> std::net::SocketAddr {
        "127.0.0.1:1111".parse().unwrap()
    }

    pub(super) fn default_disk_cache_size() -> super::ByteSize {
        super::ByteSize::mib(200)
    }

    pub(super) fn mom_base_url() -> String {
        "http://localhost:1118".to_string()
    }

    pub(super) fn mom_api_key() -> super::MomApiKey {
        eprintln!(
            "\x1b[33mWarning: Using dummy MOM_API_KEY. Set MOM_API_KEY if you want to be able to deploy.\x1b[0m"
        );
        MOM_DEV_API_KEY.to_owned()
    }

    pub(super) fn random_port_fallback() -> bool {
        true
    }
}

#[derive(Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Clone)]
pub struct ObjectStorageConfig {
    pub bucket: S3BucketName,
    pub region: S3RegionName,
    // if set, will override the region
    pub endpoint: Option<S3Endpoint>,
}

#[derive(Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Clone)]
pub struct TenantSecrets {
    pub aws: AwsSecrets,
    pub patreon: Option<PatreonSecrets>,
    pub github: Option<GitHubSecrets>,
    /// Derived cookie sauce for this tenant (derived from global secret)
    #[facet(optional)]
    pub cookie_sauce: Option<String>,
}

#[derive(Facet, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AwsSecrets {
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Environment {
    Development,
    Production,
}

use std::{collections::HashMap, net::SocketAddr, sync::LazyLock};

impl Default for Environment {
    fn default() -> Self {
        static DEFAULT_ENV: LazyLock<Environment> = LazyLock::new(|| {
            std::env::var("HOME_ENV")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Environment::Development)
        });
        *DEFAULT_ENV
    }
}

impl Environment {
    pub fn is_dev(&self) -> bool {
        matches!(self, Environment::Development)
    }

    pub fn is_prod(&self) -> bool {
        matches!(self, Environment::Production)
    }
}

impl std::str::FromStr for Environment {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "development" => Ok(Self::Development),
            "production" => Ok(Self::Production),
            _ => Err(format!("Unknown environment {s:?}")),
        }
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Development => write!(f, "development"),
            Self::Production => write!(f, "production"),
        }
    }
}

#[derive(Facet, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatreonSecrets {
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

#[derive(Facet, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubSecrets {
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

#[derive(Clone, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedditSecrets {
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

#[derive(Clone, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomSecrets {
    /// Can read all tenants — used by cubs
    pub readonly_api_key: MomApiKey,

    /// Can read/write specific tenants, used by humans
    #[serde(default)]
    pub scoped_api_keys: HashMap<MomApiKey, ScopedMomApiKey>,

    /// Global secret for deriving per-tenant cookie encryption keys
    pub cookie_sauce: String,
}

pub const MOM_DEV_API_KEY: &MomApiKeyRef = MomApiKeyRef::from_static("mom_KEY_IN_DEV");

#[derive(Clone, Facet, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedMomApiKey {
    #[serde(default)]
    pub tenants: Vec<TenantDomain>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct ByteSize(u64);

impl ByteSize {
    pub fn new(size: u64) -> Self {
        Self(size)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn mib(size: u64) -> Self {
        ByteSize(size * 1024 * 1024)
    }
}

impl std::fmt::Display for ByteSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = self.0;
        if size % (1024 * 1024 * 1024) == 0 {
            write!(f, "{} GiB", size / (1024 * 1024 * 1024))
        } else if size % (1024 * 1024) == 0 {
            write!(f, "{} MiB", size / (1024 * 1024))
        } else {
            write!(f, "{size} bytes")
        }
    }
}

impl std::str::FromStr for ByteSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_lowercase();
        if let Some(size) = s.strip_suffix(" gib") {
            size.trim()
                .parse::<u64>()
                .map(|v| ByteSize(v * 1024 * 1024 * 1024))
                .map_err(|_| format!("Invalid number in '{s}'"))
        } else if let Some(size) = s.strip_suffix(" mib") {
            size.trim()
                .parse::<u64>()
                .map(|v| ByteSize(v * 1024 * 1024))
                .map_err(|_| format!("Invalid number in '{s}'"))
        } else {
            s.parse::<u64>()
                .map(ByteSize)
                .map_err(|_| format!("Invalid number in '{s}'"))
        }
    }
}

impl<'de> serde::Deserialize<'de> for ByteSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        use std::str::FromStr;
        ByteSize::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// True if we're in development
pub fn is_development() -> bool {
    Environment::default().is_dev()
}

/// True if we're in production
pub fn is_production() -> bool {
    Environment::default().is_prod()
}

/// Returns the url of the "production" mom
pub fn production_mom_url() -> &'static str {
    "https://mom.bearcove.cloud"
}

#[cfg(test)]
mod bytesize_tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_display() {
        assert_eq!(ByteSize(1024 * 1024 * 1024).to_string(), "1 GiB");
        assert_eq!(ByteSize(1024 * 1024).to_string(), "1 MiB");
        assert_eq!(ByteSize(1024).to_string(), "1024 bytes");
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            ByteSize::from_str("1 GiB").unwrap(),
            ByteSize(1024 * 1024 * 1024)
        );
        assert_eq!(ByteSize::from_str("1 MiB").unwrap(), ByteSize(1024 * 1024));
        assert_eq!(ByteSize::from_str("1024").unwrap(), ByteSize(1024));
    }
}
