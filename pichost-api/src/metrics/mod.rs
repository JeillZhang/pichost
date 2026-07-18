use lazy_static::lazy_static;
use prometheus::{register_counter_vec, register_histogram_vec, register_int_gauge, Encoder, Registry, TextEncoder};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    pub static ref HTTP_REQUESTS_TOTAL: prometheus::CounterVec = register_counter_vec!(
        "pichost_http_requests_total",
        "Total HTTP requests",
        &["method", "path", "status"]
    )
    .unwrap();

    pub static ref HTTP_REQUEST_DURATION: prometheus::HistogramVec = register_histogram_vec!(
        "pichost_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "path"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    )
    .unwrap();

    pub static ref UPLOADS_TOTAL: prometheus::Counter = prometheus::register_counter!(
        "pichost_uploads_total",
        "Total image uploads"
    )
    .unwrap();

    pub static ref UPLOAD_ERRORS_TOTAL: prometheus::Counter = prometheus::register_counter!(
        "pichost_upload_errors_total",
        "Total upload errors"
    )
    .unwrap();

    pub static ref TOTAL_USERS: prometheus::IntGauge = register_int_gauge!(
        "pichost_users_total",
        "Total registered users"
    )
    .unwrap();

    pub static ref TOTAL_IMAGES: prometheus::IntGauge = register_int_gauge!(
        "pichost_images_total",
        "Total images"
    )
    .unwrap();

    pub static ref TOTAL_STORAGE_BYTES: prometheus::IntGauge = register_int_gauge!(
        "pichost_storage_bytes_total",
        "Total storage used in bytes"
    )
    .unwrap();
}

pub fn encode_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).expect("encode metrics");
    String::from_utf8(buffer).expect("metrics utf8")
}
