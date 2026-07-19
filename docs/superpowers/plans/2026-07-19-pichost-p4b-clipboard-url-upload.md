# PicHost P4-B: Clipboard Paste + URL Upload — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow users to upload images by pasting from clipboard (Ctrl+V) or providing an image URL, reusing the existing `process_upload()` pipeline with full storage backend selection.

**Architecture:** Backend adds a JSON endpoint `POST /api/v1/images/upload-url` that downloads the image from the provided URL with SSRF protection (DNS-level IP filtering, scheme whitelist, redirect limits), then feeds the downloaded bytes into the existing `process_upload()`. Frontend adds a `useClipboardPaste` hook monitoring `paste` events on the document, extracting `ClipboardItem` image blobs into `File` objects and feeding them into `useUploadQueue.addFiles()`. A compact `UrlUploadInput` component sits beside the DropZone, calling the new API endpoint and appending the result to the upload queue.

**Tech Stack:** Rust (Axum, reqwest, tokio, std::net), TypeScript (React 19, ky, navigator.clipboard)

## Agent Worker Instructions

- **Required sub-skills:** rust-refactor-fns (enforce ≤50-line functions, ≤120-char lines)
- **Recommended execution mode:** `subagent-driven-development` — dispatch a fresh subagent per task, review between tasks
- **Required verification:** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `npm run build` (`npx tsc --noEmit && vite build`)
- **Version bump:** `v0.15.0` → `v0.15.1` (patch — new feature, no DB migration)
- **No DB migration** — P4-B does not modify the database schema.

## Global Constraints

- Rust: functions ≤50 lines, each line ≤120 characters (enforce with `rust-refactor-fns` skill)
- No type suppression: zero `as any`, `@ts-ignore`, `@ts-expect-error`
- Follow existing `type RouteError = (StatusCode, Json<serde_json::Value>)` error pattern in backend
- Reuse `process_upload()` from `pichost-api/src/services/upload.rs` — do NOT duplicate upload logic
- Frontend: use existing `useUploadQueue().addFiles(files, storageConfigIds?)` as the integration point
- Frontend styles: use `var(--color-*)` CSS tokens, `glass-bg` + `backdrop-blur-sm` for inputs
- All copy/text in English

---

- id: T0
  title: "Add URL download service with SSRF protection"
  files:
    - pichost-api/src/services/upload_url.rs
    - pichost-api/src/services/mod.rs
  depends_on: []
  breaking: false
  ac:
    - given: "a URL pointing to a public image on the internet"
      when: "calling fetch_image_from_url()"
      then: "returns Ok((bytes, content_type_or_filename)) with the downloaded image bytes"
    - given: "a URL with scheme ftp:// or file:// or javascript:"
      when: "calling fetch_image_from_url()"
      then: "returns Err with 400 and 'unsupported URL scheme' message"
    - given: "a URL resolving to 127.0.0.1, 10.x.x.x, 172.16-31.x.x, or 192.168.x.x"
      when: "calling fetch_image_from_url()"
      then: "returns Err with 400 and 'URL resolves to a private/internal address' message"
    - given: "a URL that redirects more than 5 times"
      when: "calling fetch_image_from_url()"
      then: "returns Err with 400 and 'too many redirects' message"
    - given: "a URL returning >50 MB of data"
      when: "calling fetch_image_from_url()"
      then: "returns Err with 413 and 'response exceeds maximum size (50 MB)' message"
    - given: "a URL returning non-image content"
      when: "calling fetch_image_from_url()"
      then: "returns Err with 400 and 'downloaded content is not a valid image' message"
  regression:
    - "cargo test -p pichost-api test_image_list -- --exact"
    - "cargo test -p pichost-core -- --exact"
  test_code: |
    // pichost-api/src/services/upload_url.rs — test module

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_is_private_ip_loopback() {
            assert!(is_private_ip(&[127, 0, 0, 1]));
            assert!(is_private_ip(&[127, 255, 255, 254]));
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
        fn test_is_private_ip_multicast() {
            assert!(is_private_ip(&[224, 0, 0, 1]));
        }

        #[test]
        fn test_is_private_ip_public_addresses() {
            assert!(!is_private_ip(&[8, 8, 8, 8]));
            assert!(!is_private_ip(&[1, 1, 1, 1]));
            assert!(!is_private_ip(&[203, 0, 113, 5]));
        }

        #[test]
        fn test_is_private_ip_special_broadcast() {
            assert!(is_private_ip(&[0, 0, 0, 0]));
            assert!(is_private_ip(&[255, 255, 255, 255]));
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
            let result = validate_url_scheme("ftp://example.com/photo.jpg");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("unsupported URL scheme"));
        }

        #[test]
        fn test_validate_url_scheme_file_rejected() {
            let result = validate_url_scheme("file:///etc/passwd");
            assert!(result.is_err());
        }

        #[test]
        fn test_validate_url_scheme_javascript_rejected() {
            let result = validate_url_scheme("javascript:alert(1)");
            assert!(result.is_err());
        }

        #[test]
        fn test_extract_filename_from_url() {
            assert_eq!(extract_filename("https://example.com/photo.jpg"), "photo.jpg");
            assert_eq!(extract_filename("https://example.com/path/to/image.png?size=large"), "image.png");
            assert_eq!(extract_filename("https://example.com/noext"), "noext");
        }
    }
  impl_code: |
    // === pichost-api/src/services/upload_url.rs ===
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::StatusCode;
    use reqwest::redirect::Policy;
    use serde_json::json;

    use crate::services::upload::{self, UploadResult};
    use crate::app::AppState;
    use crate::middleware::auth::AuthUser;
    use crate::ApiError;

    const MAX_REDIRECTS: usize = 5;
    const DOWNLOAD_TIMEOUT_SECS: u64 = 30;
    const MAX_BODY_SIZE: u64 = 52_428_800; // 50 MB (matches DefaultBodyLimit)

    /// Check whether an IPv4 address belongs to a private or reserved range.
    /// Covers: loopback, class A/B/C private, link-local, multicast, broadcast,
    /// documentation (TEST-NET), carrier-grade NAT, and 0.0.0.0.
    pub fn is_private_ip(octets: &[u8; 4]) -> bool {
        match octets {
            [0, _, _, _] => true,
            [10, ..] => true,
            [127, ..] => true,
            [169, 254, ..] => true,
            [172, b, ..] if (16..=31).contains(b) => true,
            [192, 168, ..] => true,
            [224, ..] => true,
            [255, 255, 255, 255] => true,
            [100, 64..=127, ..] => true,              // 100.64.0.0/10 CGN
            [192, 0, 0, ..] => true,                   // 192.0.0.0/24 IETF
            [192, 0, 2, ..] => true,                   // TEST-NET-1
            [198, 51, 100, ..] => true,                // TEST-NET-2
            [203, 0, 113, ..] => true,                 // TEST-NET-3
            [198, 18..=19, ..] => true,                // benchmark
            _ => false,
        }
    }

    /// Validate that the URL scheme is http or https only.
    pub fn validate_url_scheme(url_str: &str) -> Result<url::Url, ApiError> {
        let parsed = url::Url::parse(url_str).map_err(|e| {
            ApiError::Upload(format!("invalid URL: {}", e))
        })?;
        match parsed.scheme() {
            "http" | "https" => Ok(parsed),
            other => Err(ApiError::Upload(format!(
                "unsupported URL scheme: {} (only http/https allowed)",
                other
            ))),
        }
    }

    /// Extract a filename from a URL path (last segment after final /, before ?).
    fn extract_filename(url: &url::Url) -> String {
        url.path_segments()
            .and_then(|segs| segs.last())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "image".to_string())
    }

    /// Resolve the host to IP addresses and check that NONE are private.
    async fn resolve_and_check_host(host: &str) -> Result<(), ApiError> {
        use tokio::net::lookup_host;
        let addrs: Vec<_> = (host, 0u16).to_socket_addrs()
            .map_err(|e| ApiError::Upload(format!("failed to resolve host: {}", e)))?;
        if addrs.is_empty() {
            return Err(ApiError::Upload("URL host resolved to zero addresses".into()));
        }
        for addr in &addrs {
            let ip = addr.ip();
            let is_private = match ip {
                IpAddr::V4(v4) => is_private_ip(&v4.octets()),
                IpAddr::V6(v6) => {
                    // Reject loopback (::1), link-local (fe80::), unique-local (fc00::/7)
                    v6.is_loopback()
                        || v6.is_multicast()
                        || v6.is_unspecified()
                        || (v6.segments()[0] & 0xfe00 == 0xfc00)
                        || (v6.segments()[0] & 0xffc0 == 0xfe80)
                }
            };
            if is_private {
                return Err(ApiError::Upload(
                    "URL resolves to a private/internal address — SSRF blocked".into(),
                ));
            }
        }
        Ok(())
    }

    /// Download an image from a URL with full SSRF protection.
    /// Returns `(bytes, filename)` on success.
    pub async fn fetch_image_from_url(url: &str) -> Result<(Vec<u8>, String), ApiError> {
        let parsed = validate_url_scheme(url)?;

        let host = parsed.host_str().ok_or_else(|| {
            ApiError::Upload("URL has no host".into())
        })?;
        resolve_and_check_host(host).await?;

        let client = reqwest::Client::builder()
            .redirect(Policy::limited(MAX_REDIRECTS))
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .build()
            .map_err(|e| ApiError::Upload(format!("failed to build HTTP client: {}", e)))?;

        let response = client.get(parsed.as_str()).send().await.map_err(|e| {
            if e.is_timeout() {
                ApiError::Upload("download timed out (30s)".into())
            } else if e.is_connect() {
                ApiError::Upload(format!("failed to connect: {}", e))
            } else if e.is_redirect() {
                ApiError::Upload("too many redirects (max 5)".into())
            } else {
                ApiError::Upload(format!("download failed: {}", e))
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Upload(format!(
                "remote server returned {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("unknown")
            )));
        }

        let content_length = response.content_length().unwrap_or(0);
        if content_length > MAX_BODY_SIZE {
            return Err(ApiError::Upload(format!(
                "response exceeds maximum size (50 MB), got: {} bytes",
                content_length
            )));
        }

        let bytes = response.bytes().await.map_err(|e| {
            ApiError::Upload(format!("failed to read response body: {}", e))
        })?;

        if bytes.len() as u64 > MAX_BODY_SIZE {
            return Err(ApiError::Upload(format!(
                "response exceeds maximum size (50 MB), got: {} bytes",
                bytes.len()
            )));
        }

        if !infer::is_image(&bytes) {
            return Err(ApiError::Upload(
                "downloaded content is not a valid image".into(),
            ));
        }

        let filename = extract_filename(&parsed);

        Ok((bytes.to_vec(), filename))
    }

    /// Full URL upload handler: download, validate, delegate to process_upload().
    pub async fn handle_url_upload(
        state: &AppState,
        user: &AuthUser,
        url: &str,
        storage_config_ids: Option<Vec<uuid::Uuid>>,
    ) -> Result<Vec<UploadResult>, ApiError> {
        let (bytes, filename) = fetch_image_from_url(url).await?;
        upload::process_upload(state, user, bytes, filename, storage_config_ids).await
    }

    // === pichost-api/src/services/mod.rs — append line ===
    pub mod upload_url;

  verify:
    - "cargo test -p pichost-api upload_url -- --exact"
    - "cargo test -p pichost-api test_is_private_ip -- --exact"
    - "cargo clippy -p pichost-api -- -D warnings"

---
- id: T1
  title: "Add URL upload route handler and register route"
  files:
    - pichost-api/src/routes/images.rs
    - pichost-api/src/main.rs
  depends_on: [T0]
  breaking: false
  ac:
    - given: "a valid JWT and a JSON body { \"url\": \"https://example.com/photo.jpg\" }"
      when: "POST /api/v1/images/upload-url"
      then: "returns 201 Created with a Vec<UploadResult>"
    - given: "a valid JWT and a JSON body { \"url\": \"https://example.com/photo.jpg\", \"storage_config_ids\": [\"uuid\"] }"
      when: "POST /api/v1/images/upload-url"
      then: "returns 201 with UploadResult for the specified storage config"
    - given: "a request without JWT"
      when: "POST /api/v1/images/upload-url"
      then: "returns 401 Unauthorized"
    - given: "a JSON body missing the url field"
      when: "POST /api/v1/images/upload-url"
      then: "returns 400 with 'missing url field' error"
    - given: "a valid JWT and a URL downloading a private IP"
      when: "POST /api/v1/images/upload-url"
      then: "returns 400 with 'private/internal address' error"
  regression:
    - "cargo test -p pichost-api test_image_list -- --exact"
    - "cargo test -p pichost-api test_upload -- --exact"
  test_code: |
    // pichost-api/src/routes/images.rs — test module additions

    // Note: Full integration tests for url_upload_handler require a running
    // server. The unit tests below validate the handler's request parsing and
    // error paths. SSRF logic is tested in T0.

    #[cfg(test)]
    mod url_upload_tests {
        use super::*;
        use axum::http::Request;
        use axum::body::Body;
        use serde_json::json;
        use tower::ServiceExt;

        // Test: UrlUploadRequest deserialization — valid payload
        #[test]
        fn test_url_upload_request_deserialize_valid() {
            let json = json!({"url": "https://example.com/photo.jpg"});
            let req: UrlUploadRequest = serde_json::from_value(json).unwrap();
            assert_eq!(req.url, "https://example.com/photo.jpg");
            assert!(req.storage_config_ids.is_none());
        }

        // Test: UrlUploadRequest deserialization — with storage_config_ids
        #[test]
        fn test_url_upload_request_deserialize_with_config_ids() {
            let json = json!({
                "url": "https://example.com/img.png",
                "storage_config_ids": ["550e8400-e29b-41d4-a716-446655440000"]
            });
            let req: UrlUploadRequest = serde_json::from_value(json).unwrap();
            assert_eq!(req.url, "https://example.com/img.png");
            assert_eq!(req.storage_config_ids.unwrap().len(), 1);
        }

        // Test: UrlUploadRequest deserialization — missing url field
        #[test]
        fn test_url_upload_request_missing_url() {
            let json = json!({"storage_config_ids": []});
            let result = serde_json::from_value::<UrlUploadRequest>(json);
            assert!(result.is_err());
        }

        // Test: UrlUploadRequest deserialization — empty url
        #[test]
        fn test_url_upload_request_empty_url() {
            let json = json!({"url": ""});
            let result = serde_json::from_value::<UrlUploadRequest>(json);
            assert!(result.is_ok());
            let req = result.unwrap();
            assert!(req.url.is_empty());
        }

        // Test: validate_url_not_empty helper
        #[test]
        fn test_validate_url_not_empty_pass() {
            assert!(validate_url_not_empty("https://example.com/img.jpg").is_ok());
        }

        // Test: validate_url_not_empty fails on empty string
        #[test]
        fn test_validate_url_not_empty_empty() {
            let result = validate_url_not_empty("");
            assert!(result.is_err());
        }
    }
  impl_code: |
    // === pichost-api/src/routes/images.rs — ADD at top (after existing imports) ===
    use crate::services::upload_url::{self, handle_url_upload};

    // ADD DTO struct near top of file (after existing type aliases, before handler functions)
    #[derive(Debug, serde::Deserialize)]
    pub struct UrlUploadRequest {
        pub url: String,
        #[serde(default)]
        pub storage_config_ids: Option<Vec<Uuid>>,
    }

    fn validate_url_not_empty(url: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
        if url.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "url field is required"})),
            ));
        }
        Ok(())
    }

    /// POST /api/v1/images/upload-url
    /// Body: { "url": "https://example.com/photo.jpg", "storage_config_ids": ["uuid1"] }
    /// Returns: 201 Created with Vec<UploadResult>
    pub async fn url_upload_handler(
        State(state): State<Arc<AppState>>,
        Extension(user): Extension<AuthUser>,
        Json(payload): Json<UrlUploadRequest>,
    ) -> Result<(StatusCode, Json<Vec<UploadResult>>), (StatusCode, Json<serde_json::Value>)> {
        validate_url_not_empty(&payload.url)?;

        match upload_url::handle_url_upload(&state, &user, &payload.url, payload.storage_config_ids).await {
            Ok(results) => {
                crate::metrics::UPLOADS_TOTAL.inc();
                Ok((StatusCode::CREATED, Json(results)))
            }
            Err(e) => {
                crate::metrics::UPLOAD_ERRORS_TOTAL.inc();
                Err(e)
            }
        }
    }

    // === pichost-api/src/main.rs — in upload_routes(), ADD new route ===

    fn upload_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        let protected = middleware::from_fn_with_state(state.clone(), require_auth);
        Router::new()
            .route("/", post(routes::images::upload_handler))
            .route("/upload-url", post(routes::images::url_upload_handler))  // <-- ADD
            .route_layer(middleware::from_fn_with_state(state, rate_limit::rate_limit_upload))
            .route_layer(protected)
    }

  verify:
    - "cargo test -p pichost-api url_upload -- --exact"
    - "cargo test -p pichost-api test_image_list -- --exact"
    - "cargo clippy --workspace -- -D warnings"

---
- id: T2
  title: "Add uploadFromUrl() to API client with types"
  files:
    - web-ui/src/api/client.ts
  depends_on: [T1]
  breaking: false
  ac:
    - given: "a valid image URL"
      when: "calling uploadFromUrl('https://example.com/photo.jpg')"
      then: "sends POST /api/v1/images/upload-url with JSON { url } and returns UploadResult"
    - given: "a URL with storage config IDs"
      when: "calling uploadFromUrl('https://example.com/img.png', ['uuid1', 'uuid2'])"
      then: "sends JSON { url, storage_config_ids: ['uuid1', 'uuid2'] }"
    - given: "a 400 error from the backend"
      when: "calling uploadFromUrl()"
      then: "throws with the error message from the backend response"
  regression:
    - "cd web-ui && npx tsc --noEmit"
  test_code: |
    // No frontend test infrastructure exists (no Vitest/Jest configured).
    // Verification via: cd web-ui && npx tsc --noEmit
  impl_code: |
    // === web-ui/src/api/client.ts — ADD after uploadImage() (line ~193) ===

    export interface UrlUploadRequest {
      url: string
      storage_config_ids?: string[]
    }

    export async function uploadFromUrl(
      url: string,
      storageConfigIds?: string[],
    ): Promise<UploadResult> {
      const body: UrlUploadRequest = { url }
      if (storageConfigIds?.length) {
        body.storage_config_ids = storageConfigIds
      }
      return api.post('images/upload-url', { json: body }).json<UploadResult>()
    }

  verify:
    - "cd web-ui && npx tsc --noEmit"

---
- id: T3
  title: "Create useClipboardPaste hook for image paste events"
  files:
    - web-ui/src/hooks/useClipboardPaste.ts
  depends_on: []
  breaking: false
  ac:
    - given: "a user presses Ctrl+V with an image in the clipboard"
      when: "the paste event fires"
      then: "onPaste callback is called with a File object extracted from the clipboard"
    - given: "a user presses Ctrl+V with only text in the clipboard"
      when: "the paste event fires"
      then: "onPaste is NOT called (ignores non-image clipboard items)"
    - given: "the component using the hook unmounts"
      when: "cleanup runs"
      then: "the paste event listener is removed from the document"
  regression:
    - "cd web-ui && npm run build"
  test_code: |
    // No frontend test infrastructure exists (no Vitest/Jest configured).
    // Verification via: cd web-ui && npx tsc --noEmit && npm run build
  impl_code: |
    // === web-ui/src/hooks/useClipboardPaste.ts (CREATE) ===
    import { useEffect } from 'react'

    /**
     * Listens for `paste` events on the document and extracts image files
     * from the clipboard. Calls `onPaste(files)` with File[] when image
     * data is found in the clipboard.
     *
     * Only the first image ClipboardItem is processed. Non-image paste
     * events (text, files, etc.) are silently ignored.
     */
    export function useClipboardPaste(onPaste: (files: File[]) => void) {
      useEffect(() => {
        const handler = (e: ClipboardEvent) => {
          const items = e.clipboardData?.items
          if (!items) return

          for (let i = 0; i < items.length; i++) {
            const item = items[i]
            if (item.kind === 'file' && item.type.startsWith('image/')) {
              const file = item.getAsFile()
              if (file) {
                onPaste([file])
                break // Only process the first image
              }
            }
          }
        }

        document.addEventListener('paste', handler)
        return () => document.removeEventListener('paste', handler)
      }, [onPaste])
    }

  verify:
    - "cd web-ui && npx tsc --noEmit"

---
- id: T4
  title: "Create UrlUploadInput component and integrate into Dashboard"
  files:
    - web-ui/src/components/UrlUploadInput.tsx
    - web-ui/src/pages/Dashboard.tsx
  depends_on: [T2, T3]
  breaking: false
  ac:
    - given: "the Dashboard page is loaded"
      when: "a user enters a URL and clicks Upload or presses Enter"
      then: "the uploadFromUrl API is called and the result appears in the upload queue"
    - given: "the Dashboard page is loaded"
      when: "a user presses Ctrl+V with an image in clipboard"
      then: "the image is added to the upload queue via addFiles()"
    - given: "the Dashboard page is loaded"
      when: "the URL input is empty"
      then: "the Upload button is disabled"
    - given: "a URL upload is in progress"
      when: "polling state"
      then: "the input and button are disabled with a 'Downloading...' placeholder"
  regression:
    - "cd web-ui && npm run build"
  test_code: |
    // No frontend test infrastructure exists (no Vitest/Jest configured).
    // Verification via: cd web-ui && npm run build
  impl_code: |
    // === web-ui/src/components/UrlUploadInput.tsx (CREATE) ===
    import { useState } from 'react'
    import { Link } from 'lucide-react'

    interface UrlUploadInputProps {
      onUpload: (url: string) => Promise<void>
    }

    export default function UrlUploadInput({ onUpload }: UrlUploadInputProps) {
      const [url, setUrl] = useState('')
      const [loading, setLoading] = useState(false)

      const handleSubmit = async () => {
        const trimmed = url.trim()
        if (!trimmed || loading) return
        setLoading(true)
        try {
          await onUpload(trimmed)
          setUrl('')
        } finally {
          setLoading(false)
        }
      }

      const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          handleSubmit()
        }
      }

      return (
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Link className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--color-text-muted)]" />
            <input
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Paste image URL..."
              disabled={loading}
              className="w-full rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] py-1.5 pl-9 pr-3 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-muted)] backdrop-blur-sm focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-50"
            />
          </div>
          <button
            onClick={handleSubmit}
            disabled={!url.trim() || loading}
            className="shrink-0 rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {loading ? '...' : 'Upload'}
          </button>
        </div>
      )
    }

    // === web-ui/src/pages/Dashboard.tsx — MODIFICATIONS ===

    // ADD imports at top (after existing imports):
    import { useClipboardPaste } from '../hooks/useClipboardPaste'
    import { uploadFromUrl } from '../api/client'
    import UrlUploadInput from '../components/UrlUploadInput'

    // ADD paste handler inside Dashboard component (after handleUpload definition, ~line 51):
    const handlePaste = useCallback(
      (files: File[]) => {
        addFiles(files, selectedConfigIds.length > 0 ? selectedConfigIds : undefined)
      },
      [addFiles, selectedConfigIds],
    )
    useClipboardPaste(handlePaste)

    // ADD URL upload handler (after handlePaste, before return):
    const handleUrlUpload = useCallback(
      async (url: string) => {
        // Use the API client which handles auth + error responses
        await uploadFromUrl(
          url,
          selectedConfigIds.length > 0 ? selectedConfigIds : undefined,
        )
        // Invalidate the images list to show the newly uploaded image
        queryClient.invalidateQueries({ queryKey: ['images'] })
      },
      [selectedConfigIds, queryClient],
    )

    // Note: useCallback requires adding to the import from 'react' at line 1:
    // import { useRef, useEffect, useState, useCallback } from 'react'

    // ADD UrlUploadInput after DropZone (after line 174 `<DropZone onUpload={handleUpload} />`):
    <div className="mt-3">
      <UrlUploadInput onUpload={handleUrlUpload} />
    </div>

  verify:
    - "cd web-ui && npx tsc --noEmit"
    - "cd web-ui && npm run build"

---
- id: T5
  title: "Run full verification suite and bump version"
  files: []
  depends_on: [T0, T1, T2, T3, T4]
  breaking: false
  ac:
    - given: "all code changes are complete"
      when: "running cargo test --workspace"
      then: "all tests pass (existing + new upload_url tests)"
    - given: "all code changes are complete"
      when: "running cargo clippy --workspace -- -D warnings"
      then: "zero warnings"
    - given: "all code changes are complete"
      when: "running npm run build"
      then: "TypeScript compilation and Vite build succeed"
  regression:
    - "cargo test --workspace"
    - "cargo clippy --workspace -- -D warnings"
    - "cd web-ui && npm run build"
  test_code: |
    // No new test code — verification step only.
    // Run: cargo test --workspace
    // Run: cargo clippy --workspace -- -D warnings
    // Run: cd web-ui && npm run build
  impl_code: |
    No implementation — verification step only.

    Steps:
    1. cargo test --workspace
    2. cargo clippy --workspace -- -D warnings
    3. cd web-ui && npx tsc --noEmit && npm run build
    4. Bump version in all Cargo.toml files: 0.15.0 → 0.15.1
    5. Bump version in web-ui/package.json: 0.15.0 → 0.15.1
    6. cargo check --workspace (confirm version consistency)

  verify:
    - "cargo test --workspace"
    - "cargo clippy --workspace -- -D warnings"
    - "cd web-ui && npm run build"
