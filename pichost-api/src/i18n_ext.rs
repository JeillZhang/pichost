use axum::extract::{FromRequest, FromRequestParts};
use axum::http::header::ACCEPT_LANGUAGE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use pichost_core::i18n::{I18n, Language};
use serde_json::Value;

pub fn locale_from_header(value: Option<&HeaderValue>, fallback: Language) -> Language {
    let Some(v) = value else { return fallback; };
    v.to_str()
        .ok()
        .and_then(|s| {
            s.split(',')
                .map(|p| p.trim().split(';').next().unwrap_or(""))
                .find_map(|tag| match tag.trim().to_ascii_lowercase().as_str() {
                    "zh" | "zh-cn" => Some(Language::ZhCN),
                    "en" | "en-us" => Some(Language::En),
                    _ => None,
                })
        })
        .unwrap_or(fallback)
}

#[derive(Debug, Clone, Copy)]
pub struct Locale(pub Language);
impl Locale {
    pub fn from_parts(headers: &HeaderMap) -> Self {
        let fallback = I18n::global().language();
        Self(locale_from_header(headers.get(ACCEPT_LANGUAGE), fallback))
    }
}
impl FromRequestParts<crate::app::AppState> for Locale {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &crate::app::AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_parts(&parts.headers))
    }
}

impl FromRequestParts<std::sync::Arc<crate::app::AppState>> for Locale {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &std::sync::Arc<crate::app::AppState>,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_parts(&parts.headers))
    }
}

fn envelope(locale: Language, key: &str, args: &[String], extra: Value) -> Value {
    let msg = if args.is_empty() {
        I18n::global().t(locale, key)
    } else {
        I18n::global().t_args(locale, key, args)
    };
    let mut v = serde_json::json!({ "error": msg, "code": key });
    if let Some(o) = v.as_object_mut() {
        if let Some(x) = extra.as_object() {
            o.extend(x.clone());
        }
    }
    v
}
pub fn error_json(locale: Language, status: StatusCode, key: &str) -> (StatusCode, Json<Value>) {
    (status, Json(envelope(locale, key, &[], serde_json::json!({}))))
}
pub fn error_json_args(
    locale: Language,
    status: StatusCode,
    key: &str,
    args: &[String],
) -> (StatusCode, Json<Value>) {
    (status, Json(envelope(locale, key, args, serde_json::json!({}))))
}
pub fn error_json_extra(
    locale: Language,
    status: StatusCode,
    key: &str,
    extra: Value,
) -> (StatusCode, Json<Value>) {
    (status, Json(envelope(locale, key, &[], extra)))
}

#[derive(Debug)]
pub struct JsonBody<T>(pub T);
impl<T, S> FromRequest<S> for JsonBody<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = axum::response::Response;
    async fn from_request(
        req: axum::extract::Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let fallback = I18n::global().language();
        let locale = locale_from_header(req.headers().get(ACCEPT_LANGUAGE), fallback);
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(JsonBody(v)),
            Err(rejection) => Err(
                error_json(locale, rejection.status(), "validation.body_invalid").into_response(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, StatusCode};
    use pichost_core::i18n::{I18n, Language};

    use super::{error_json, locale_from_header, JsonBody};

    #[test]
    fn locale_from_header_resolution() {
        assert_eq!(
            locale_from_header(Some(&HeaderValue::from_static("zh-CN,zh;q=0.9")), Language::En),
            Language::ZhCN
        );
        assert_eq!(
            locale_from_header(Some(&HeaderValue::from_static("fr-FR,fr;q=0.9")), Language::En),
            Language::En
        );
        assert_eq!(locale_from_header(None, Language::En), Language::En);
    }

    #[test]
    fn error_json_envelope_shape() {
        I18n::init_global(Language::En, None);
        let (status, body) = error_json(
            Language::ZhCN,
            StatusCode::UNAUTHORIZED,
            "validation.invalid_credentials",
        );
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body.0["error"], "用户名或密码错误");
        assert_eq!(body.0["code"], "validation.invalid_credentials");
    }

    #[tokio::test]
    async fn json_body_rejection_localized() {
        use axum::extract::FromRequest;
        use axum::http::Request;
        use axum::response::IntoResponse;
        I18n::init_global(Language::ZhCN, None);
        let req = Request::builder()
            .header("content-type", "application/json")
            .header("accept-language", "zh-CN")
            .body(axum::body::Body::from("{\"bad json"))
            .unwrap();
        let resp = JsonBody::<serde_json::Value>::from_request(req, &())
            .await
            .unwrap_err()
            .into_response();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["code"], "validation.body_invalid");
        assert_eq!(v["error"], "请求体无效");
    }
}
