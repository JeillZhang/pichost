// pichost-api/tests/static_serve_test.rs — 新建(无 DB,纯 Router 单测)
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;
use pichost_api::app::mount_static_fallback;
use std::path::Path;
use tower::ServiceExt;

async fn body_text(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn serves_spa_with_fallback_and_route_priority() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<html>SPA</html>").unwrap();
    std::fs::create_dir(tmp.path().join("assets")).unwrap();
    std::fs::write(tmp.path().join("assets/app.js"), "console.log(1)").unwrap();

    let router = Router::new().route("/api/v1/ping", get(|| async { "pong" }));
    let router = mount_static_fallback(router, tmp.path());

    let req = |uri: &str| {
        router
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
    };
    let resp = req("/").await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_text(resp).await.contains("SPA"));

    let resp = req("/assets/app.js").await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_text(resp).await, "console.log(1)");

    let resp = req("/missing").await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_text(resp).await.contains("SPA"));

    let resp = req("/api/v1/ping").await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_text(resp).await, "pong");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_dir_skips_mount() {
    let router = Router::new().route("/api/v1/ping", get(|| async { "pong" }));
    let router = mount_static_fallback(router, Path::new("/nonexistent-dist-xyz"));
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/index.html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
