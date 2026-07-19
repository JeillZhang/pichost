use std::net::IpAddr;
use std::time::Duration;

use axum::http::StatusCode;
use axum::Json;
use reqwest::redirect::Policy;
use serde_json::json;

type ApiError = (StatusCode, Json<serde_json::Value>);

const MAX_REDIRECTS: usize = 5;
const DOWNLOAD_TIMEOUT_SECS: u64 = 30;
const MAX_BODY_SIZE: u64 = 52_428_800; // 50 MB

fn err(msg: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"error": msg.into()})))
}

/// Check whether an IPv4 address belongs to a private or reserved range.
pub fn is_private_ip(octets: &[u8; 4]) -> bool {
    #[allow(clippy::match_like_matches_macro)]
    match octets {
        [0, ..] => true,                                       // 0.0.0.0/8
        [10, ..] => true,                                      // 10.0.0.0/8
        [127, ..] => true,                                     // 127.0.0.0/8
        [169, 254, ..] => true,                                // 169.254.0.0/16
        [172, b, ..] if (16..=31).contains(b) => true,         // 172.16.0.0/12
        [192, 168, ..] => true,                                // 192.168.0.0/16
        [224..=239, ..] => true,                               // multicast
        [255, 255, 255, 255] => true,                          // broadcast
        [100, 64..=127, ..] => true,                           // 100.64.0.0/10
        [192, 0, 0, ..] => true,                               // 192.0.0.0/24
        [192, 0, 2, ..] => true,                               // TEST-NET-1
        [198, 51, 100, ..] => true,                            // TEST-NET-2
        [203, 0, 113, ..] => true,                             // TEST-NET-3
        [198, 18..=19, ..] => true,                            // benchmark
        _ => false,
    }
}

/// Validate that the URL uses an allowed scheme (http or https only).
pub fn validate_url_scheme(url_str: &str) -> Result<url::Url, ApiError> {
    let parsed = url::Url::parse(url_str).map_err(|e| err(format!("invalid URL: {}", e)))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        other => Err(err(format!(
            "unsupported URL scheme: {} (only http/https allowed)",
            other
        ))),
    }
}

/// Extract a filename from a URL path (last segment after final `/`, before `?`).
fn extract_filename_from_url_str(url: &url::Url) -> String {
    url.path_segments()
        .and_then(|mut segs| segs.next_back())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "image".to_string())
}

/// Resolve host to IP addresses and check that NONE are private/internal.
async fn resolve_and_check_host(host: &str) -> Result<(), ApiError> {
    use std::net::ToSocketAddrs;

    let addrs: Vec<_> = format!("{}:0", host)
        .to_socket_addrs()
        .map_err(|e| err(format!("failed to resolve host: {}", e)))?
        .collect();

    if addrs.is_empty() {
        return Err(err("URL host resolved to zero addresses"));
    }

    for addr in &addrs {
        let ip = addr.ip();
        let is_private = match ip {
            IpAddr::V4(v4) => is_private_ip(&v4.octets()),
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_multicast()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00 == 0xfc00)
                    || (v6.segments()[0] & 0xffc0 == 0xfe80)
            }
        };
        if is_private {
            return Err(err(
                "URL resolves to a private/internal address — SSRF blocked",
            ));
        }
    }
    Ok(())
}

/// Download an image from a URL with full SSRF protection.
///
/// Returns `(bytes, filename)` on success.
pub async fn fetch_image_from_url(url: &str) -> Result<(Vec<u8>, String), ApiError> {
    let parsed = validate_url_scheme(url)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| err("URL has no host"))?;
    resolve_and_check_host(host).await?;

    let client = reqwest::Client::builder()
        .redirect(Policy::limited(MAX_REDIRECTS))
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| err(format!("failed to build HTTP client: {}", e)))?;

    let response = client.get(parsed.as_str()).send().await.map_err(|e| {
        if e.is_timeout() {
            err("download timed out (30s)")
        } else if e.is_connect() {
            err(format!("failed to connect: {}", e))
        } else if e.is_redirect() {
            err("too many redirects (max 5)")
        } else {
            err(format!("download failed: {}", e))
        }
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(err(format!(
            "remote server returned {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("unknown")
        )));
    }

    let content_length = response.content_length().unwrap_or(0);
    if content_length > MAX_BODY_SIZE {
        return Err(err(format!(
            "response exceeds maximum size (50 MB), got {} bytes",
            content_length
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| err(format!("failed to read body: {}", e)))?;

    if bytes.len() as u64 > MAX_BODY_SIZE {
        return Err(err(format!(
            "response exceeds maximum size (50 MB), got {} bytes",
            bytes.len()
        )));
    }

    if !infer::is_image(&bytes) {
        return Err(err("downloaded content is not a valid image"));
    }

    let filename = extract_filename_from_url_str(&parsed);
    Ok((bytes.to_vec(), filename))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private_ip_loopback() {
        assert!(is_private_ip(&[127, 0, 0, 1]));
    }

    #[test]
    fn test_is_private_ip_class_a() {
        assert!(is_private_ip(&[10, 0, 0, 1]));
        assert!(is_private_ip(&[10, 255, 255, 254]));
    }

    #[test]
    fn test_is_private_ip_class_b() {
        assert!(is_private_ip(&[172, 16, 0, 1]));
        assert!(is_private_ip(&[172, 31, 255, 254]));
    }

    #[test]
    fn test_is_private_ip_class_c() {
        assert!(is_private_ip(&[192, 168, 0, 1]));
        assert!(is_private_ip(&[192, 168, 255, 254]));
    }

    #[test]
    fn test_is_private_ip_link_local() {
        assert!(is_private_ip(&[169, 254, 0, 1]));
        assert!(is_private_ip(&[169, 254, 255, 254]));
    }

    #[test]
    fn test_is_private_ip_public_addresses() {
        assert!(!is_private_ip(&[8, 8, 8, 8]));
        assert!(!is_private_ip(&[1, 1, 1, 1]));
        assert!(!is_private_ip(&[93, 184, 216, 34])); // example.com
    }

    #[test]
    fn test_validate_url_scheme_https() {
        assert!(validate_url_scheme("https://example.com/photo.jpg").is_ok());
    }

    #[test]
    fn test_validate_url_scheme_http() {
        assert!(validate_url_scheme("http://example.com/photo.jpg").is_ok());
    }

    #[test]
    fn test_validate_url_scheme_ftp_rejected() {
        assert!(validate_url_scheme("ftp://example.com/photo.jpg").is_err());
    }

    #[test]
    fn test_validate_url_scheme_file_rejected() {
        assert!(validate_url_scheme("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_extract_filename_from_url() {
        let u1 = url::Url::parse("https://example.com/photo.jpg").unwrap();
        assert_eq!(extract_filename_from_url_str(&u1), "photo.jpg");

        let u2 = url::Url::parse("https://example.com/path/to/image.png?size=large").unwrap();
        assert_eq!(extract_filename_from_url_str(&u2), "image.png");

        let u3 = url::Url::parse("https://example.com/noext").unwrap();
        assert_eq!(extract_filename_from_url_str(&u3), "noext");
    }
}
