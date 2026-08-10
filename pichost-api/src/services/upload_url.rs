use std::net::IpAddr;
use std::time::Duration;

use axum::http::StatusCode;
use axum::Json;
use pichost_core::i18n::Language;
use reqwest::redirect::Policy;

use crate::i18n_ext::{error_json, error_json_args};

type ApiError = (StatusCode, Json<serde_json::Value>);

const MAX_REDIRECTS: usize = 5;
const DOWNLOAD_TIMEOUT_SECS: u64 = 30;
const MAX_BODY_SIZE: u64 = 52_428_800; // 50 MB

fn err(locale: Language, key: &str, args: &[String]) -> ApiError {
    if args.is_empty() {
        error_json(locale, StatusCode::BAD_REQUEST, key)
    } else {
        error_json_args(locale, StatusCode::BAD_REQUEST, key, args)
    }
}

/// Check whether an IPv4 address belongs to a private or reserved range.
pub fn is_private_ip(octets: &[u8; 4]) -> bool {
    #[allow(clippy::match_like_matches_macro)]
    match octets {
        [0, ..] => true,                               // 0.0.0.0/8
        [10, ..] => true,                              // 10.0.0.0/8
        [127, ..] => true,                             // 127.0.0.0/8
        [169, 254, ..] => true,                        // 169.254.0.0/16
        [172, b, ..] if (16..=31).contains(b) => true, // 172.16.0.0/12
        [192, 168, ..] => true,                        // 192.168.0.0/16
        [224..=239, ..] => true,                       // multicast
        [255, 255, 255, 255] => true,                  // broadcast
        [100, 64..=127, ..] => true,                   // 100.64.0.0/10
        [192, 0, 0, ..] => true,                       // 192.0.0.0/24
        [192, 0, 2, ..] => true,                       // TEST-NET-1
        [198, 51, 100, ..] => true,                    // TEST-NET-2
        [203, 0, 113, ..] => true,                     // TEST-NET-3
        [198, 18..=19, ..] => true,                    // benchmark
        _ => false,
    }
}

/// Validate that the URL uses an allowed scheme (http or https only).
pub fn validate_url_scheme(url_str: &str, locale: Language) -> Result<url::Url, ApiError> {
    let parsed =
        url::Url::parse(url_str).map_err(|e| err(locale, "url.invalid", &[e.to_string()]))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        other => Err(err(locale, "url.unsupported_scheme", &[other.to_string()])),
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
async fn resolve_and_check_host(host: &str, locale: Language) -> Result<(), ApiError> {
    use std::net::ToSocketAddrs;

    let addrs: Vec<_> = format!("{}:0", host)
        .to_socket_addrs()
        .map_err(|e| err(locale, "url.resolve_failed", &[e.to_string()]))?
        .collect();

    if addrs.is_empty() {
        return Err(err(locale, "url.no_addresses", &[]));
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
            return Err(err(locale, "url.ssrf_blocked", &[]));
        }
    }
    Ok(())
}

fn build_download_client(locale: Language) -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .redirect(Policy::limited(MAX_REDIRECTS))
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| err(locale, "url.client_failed", &[e.to_string()]))
}

async fn fetch_remote_body(
    client: &reqwest::Client,
    url: &str,
    locale: Language,
) -> Result<Vec<u8>, ApiError> {
    let response = client.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            err(locale, "url.timed_out", &[])
        } else if e.is_connect() {
            err(locale, "url.connect_failed", &[e.to_string()])
        } else if e.is_redirect() {
            err(locale, "url.too_many_redirects", &[])
        } else {
            err(locale, "url.download_failed", &[e.to_string()])
        }
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(err(
            locale,
            "url.server_error",
            &[
                status.as_u16().to_string(),
                status.canonical_reason().unwrap_or("unknown").to_string(),
            ],
        ));
    }

    let content_length = response.content_length().unwrap_or(0);
    if content_length > MAX_BODY_SIZE {
        return Err(err(locale, "url.too_large", &[content_length.to_string()]));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| err(locale, "url.body_failed", &[e.to_string()]))?;

    if bytes.len() as u64 > MAX_BODY_SIZE {
        return Err(err(locale, "url.too_large", &[bytes.len().to_string()]));
    }

    if !infer::is_image(&bytes) {
        return Err(err(locale, "url.not_an_image", &[]));
    }

    Ok(bytes.to_vec())
}

/// Download an image from a URL with full SSRF protection.
///
/// Returns `(bytes, filename)` on success.
pub async fn fetch_image_from_url(
    url: &str,
    locale: Language,
) -> Result<(Vec<u8>, String), ApiError> {
    let parsed = validate_url_scheme(url, locale)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| err(locale, "url.no_host", &[]))?;
    resolve_and_check_host(host, locale).await?;

    let client = build_download_client(locale)?;
    let bytes = fetch_remote_body(&client, parsed.as_str(), locale).await?;
    let filename = extract_filename_from_url_str(&parsed);
    Ok((bytes, filename))
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
        assert!(validate_url_scheme("https://example.com/photo.jpg", Language::En).is_ok());
    }

    #[test]
    fn test_validate_url_scheme_http() {
        assert!(validate_url_scheme("http://example.com/photo.jpg", Language::En).is_ok());
    }

    #[test]
    fn test_validate_url_scheme_ftp_rejected() {
        assert!(validate_url_scheme("ftp://example.com/photo.jpg", Language::En).is_err());
    }

    #[test]
    fn test_validate_url_scheme_file_rejected() {
        assert!(validate_url_scheme("file:///etc/passwd", Language::En).is_err());
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

    #[test]
    fn test_is_private_ip_reserved_ranges() {
        assert!(is_private_ip(&[100, 64, 0, 1]));
        assert!(is_private_ip(&[100, 127, 255, 254]));
        assert!(!is_private_ip(&[100, 128, 0, 1]));
        assert!(is_private_ip(&[192, 0, 0, 1]));
        assert!(is_private_ip(&[192, 0, 2, 1]));
        assert!(is_private_ip(&[198, 51, 100, 1]));
        assert!(is_private_ip(&[203, 0, 113, 1]));
        assert!(is_private_ip(&[198, 18, 0, 1]));
        assert!(is_private_ip(&[198, 19, 255, 255]));
        assert!(is_private_ip(&[255, 255, 255, 255]));
        assert!(is_private_ip(&[0, 0, 0, 0]));
        assert!(is_private_ip(&[224, 0, 0, 1]));
    }

    #[test]
    fn test_validate_url_scheme_no_scheme() {
        assert!(validate_url_scheme("example.com/photo.jpg", Language::En).is_err());
    }

    #[test]
    fn test_err_helper_returns_400() {
        let (status, json) = err(Language::En, "url.no_host", &[]);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json.0["code"], "url.no_host");
        assert_eq!(json.0["error"], "URL has no host");
    }

    #[test]
    fn test_extract_filename_trailing_slash_and_root() {
        let trailing = url::Url::parse("https://example.com/path/").unwrap();
        assert_eq!(extract_filename_from_url_str(&trailing), "image");
        let root = url::Url::parse("https://example.com").unwrap();
        assert_eq!(extract_filename_from_url_str(&root), "image");
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00";

    async fn spawn_server(
        status: &'static str,
        content_length: Option<u64>,
        body: &'static [u8],
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let clen = content_length.unwrap_or(body.len() as u64);
            let head = format!(
                "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                status, clen
            );
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(body).await;
        });
        format!("http://{}/image.png", addr)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_resolve_and_check_host_localhost_blocked() {
        let result = resolve_and_check_host("localhost", Language::En).await;
        let (status, json) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json.0["error"].as_str().unwrap().contains("SSRF"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_build_download_client_ok() {
        assert!(build_download_client(Language::En).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fetch_remote_body_happy_path() {
        let url = spawn_server("200 OK", None, PNG).await;
        let client = build_download_client(Language::En).unwrap();
        let bytes = fetch_remote_body(&client, &url, Language::En)
            .await
            .unwrap();
        assert_eq!(bytes, PNG);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fetch_remote_body_garbage() {
        let url = spawn_server("200 OK", None, b"not an image").await;
        let client = build_download_client(Language::En).unwrap();
        let (status, json) = fetch_remote_body(&client, &url, Language::En)
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json.0["error"]
            .as_str()
            .unwrap()
            .contains("not a valid image"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fetch_remote_body_404() {
        let url = spawn_server("404 Not Found", None, b"nope").await;
        let client = build_download_client(Language::En).unwrap();
        let (_, json) = fetch_remote_body(&client, &url, Language::En)
            .await
            .unwrap_err();
        assert!(json.0["error"].as_str().unwrap().contains("404"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fetch_remote_body_oversized() {
        let url = spawn_server("200 OK", Some(MAX_BODY_SIZE + 1), b"x").await;
        let client = build_download_client(Language::En).unwrap();
        let (_, json) = fetch_remote_body(&client, &url, Language::En)
            .await
            .unwrap_err();
        assert!(json.0["error"].as_str().unwrap().contains("maximum size"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fetch_image_from_url_rejects_private_ip() {
        let result = fetch_image_from_url("http://127.0.0.1:1/x.png", Language::En).await;
        let (status, json) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json.0["error"].as_str().unwrap().contains("SSRF"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fetch_image_from_url_rejects_ftp() {
        assert!(
            fetch_image_from_url("ftp://example.com/x.png", Language::En)
                .await
                .is_err()
        );
    }
}
