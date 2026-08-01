use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec_with_registry, register_histogram_vec_with_registry,
    register_int_gauge_with_registry, Encoder, Registry, TextEncoder,
};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref HTTP_REQUESTS_TOTAL: prometheus::CounterVec =
        register_counter_vec_with_registry!(
            "pichost_http_requests_total",
            "Total HTTP requests",
            &["method", "path", "status"],
            REGISTRY.clone()
        )
        .unwrap();
    pub static ref HTTP_REQUEST_DURATION: prometheus::HistogramVec =
        register_histogram_vec_with_registry!(
            "pichost_http_request_duration_seconds",
            "HTTP request duration in seconds",
            &["method", "path"],
            vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
            REGISTRY.clone()
        )
        .unwrap();
    pub static ref UPLOADS_TOTAL: prometheus::Counter =
        prometheus::register_counter_with_registry!(
            "pichost_uploads_total",
            "Total image uploads",
            REGISTRY.clone()
        )
        .unwrap();
    pub static ref UPLOAD_ERRORS_TOTAL: prometheus::Counter =
        prometheus::register_counter_with_registry!(
            "pichost_upload_errors_total",
            "Total upload errors",
            REGISTRY.clone()
        )
        .unwrap();
    pub static ref TOTAL_USERS: prometheus::IntGauge = register_int_gauge_with_registry!(
        "pichost_users_total",
        "Total registered users",
        REGISTRY.clone()
    )
    .unwrap();
    pub static ref TOTAL_IMAGES: prometheus::IntGauge =
        register_int_gauge_with_registry!("pichost_images_total", "Total images", REGISTRY.clone())
            .unwrap();
    pub static ref TOTAL_STORAGE_BYTES: prometheus::IntGauge = register_int_gauge_with_registry!(
        "pichost_storage_bytes_total",
        "Total storage used in bytes",
        REGISTRY.clone()
    )
    .unwrap();
}

pub fn encode_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = vec![];
    encoder
        .encode(&metric_families, &mut buffer)
        .expect("encode metrics");
    String::from_utf8(buffer).expect("metrics utf8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_metrics_contains_request_counter() {
        HTTP_REQUESTS_TOTAL.with_label_values(&["GET", "/x", "200"]).inc();
        HTTP_REQUEST_DURATION.with_label_values(&["GET", "/x"]).observe(0.1);
        let output = encode_metrics();
        assert!(output.contains("pichost_http_requests_total"));
        assert!(output.contains("pichost_http_request_duration_seconds"));
    }

    #[test]
    fn test_metrics_families_exist() {
        HTTP_REQUEST_DURATION.with_label_values(&["POST", "/y"]).observe(0.1);
        UPLOADS_TOTAL.inc();
        UPLOAD_ERRORS_TOTAL.inc();
        TOTAL_USERS.set(1);
        TOTAL_IMAGES.set(2);
        TOTAL_STORAGE_BYTES.set(3);
        let output = encode_metrics();
        assert!(output.contains("pichost_uploads_total"));
        assert!(output.contains("pichost_upload_errors_total"));
        assert!(output.contains("pichost_users_total"));
        assert!(output.contains("pichost_images_total"));
        assert!(output.contains("pichost_storage_bytes_total"));
    }
}
