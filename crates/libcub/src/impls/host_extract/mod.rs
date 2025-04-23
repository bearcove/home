use axum::http::{
    Uri,
    header::{self, FORWARDED, HeaderMap},
};

use super::{DomainResolution, global_state};
use config::TenantDomain;

const X_FORWARDED_HOST_HEADER_KEY: &str = "X-Forwarded-Host";

/// Extractor that resolves the hostname of the request.
///
/// Hostname is resolved through the following, in order:
/// - `Forwarded` header
/// - `X-Forwarded-Host` header
/// - `Host` header
/// - request target / URI
///
/// Note that user agents can set `X-Forwarded-Host` and `Host` headers to arbitrary values so make
/// sure to validate them to avoid security issues.
#[derive(Debug, Clone)]
pub struct ExtractedHost(pub String);

impl ExtractedHost {
    pub fn from_headers(uri: &Uri, headers: &HeaderMap) -> Option<Self> {
        if let Some(host) = parse_forwarded(headers) {
            return Some(ExtractedHost(host.to_owned()));
        }

        if let Some(host) = headers
            .get(X_FORWARDED_HOST_HEADER_KEY)
            .and_then(|host| host.to_str().ok())
        {
            return Some(ExtractedHost(host.to_owned()));
        }

        if let Some(host) = headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
        {
            return Some(ExtractedHost(host.to_owned()));
        }

        if let Some(host) = uri.host() {
            return Some(ExtractedHost(host.to_owned()));
        }

        None
    }

    pub fn domain(&self) -> &str {
        self.0.split(':').next().unwrap_or_default()
    }

    /// Get domain resolution for this domain
    pub fn resolve_domain(&self) -> Option<DomainResolution> {
        let domain = TenantDomain::new(self.domain().to_string());
        global_state()
            .dynamic
            .read()
            .domain_resolution
            .get(&domain)
            .cloned()
    }
}

#[allow(warnings)]
fn parse_forwarded(headers: &HeaderMap) -> Option<&str> {
    // if there are multiple `Forwarded` `HeaderMap::get` will return the first one
    let forwarded_values = headers.get(FORWARDED)?.to_str().ok()?;

    // get the first set of values
    let first_value = forwarded_values.split(',').nth(0)?;

    // find the value of the `host` field
    first_value.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("host")
            .then(|| value.trim().trim_matches('"'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain() {
        let host = ExtractedHost("example.com".to_string());
        assert_eq!(host.domain(), "example.com");

        let host_with_port = ExtractedHost("example.com:8080".to_string());
        assert_eq!(host_with_port.domain(), "example.com");

        let ip_host = ExtractedHost("192.168.1.1".to_string());
        assert_eq!(ip_host.domain(), "192.168.1.1");

        let ip_host_with_port = ExtractedHost("192.168.1.1:8080".to_string());
        assert_eq!(ip_host_with_port.domain(), "192.168.1.1");

        let empty_host = ExtractedHost("".to_string());
        assert_eq!(empty_host.domain(), "");
    }
}
