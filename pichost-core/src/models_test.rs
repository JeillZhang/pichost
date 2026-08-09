use crate::models::TaskPayload;
use uuid::Uuid;

#[test]
fn task_payload_roundtrips_json() {
    let p = TaskPayload {
        task_id: Uuid::new_v4(),
        image_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        storage_backend: "local".into(),
        storage_config_id: None,
        storage_backend_name: "local".into(),
        source_key: "k".into(),
        source_mime: "image/png".into(),
        retry_count: 0,
        max_retries: 3,
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: TaskPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back.task_id, p.task_id);
}
