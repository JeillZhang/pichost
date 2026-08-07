use axum::extract::FromRequestParts;
use axum::http::header::ACCEPT_LANGUAGE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::Json;
use pichost_core::config::AppConfig;
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
    pub fn from_parts(headers: &HeaderMap, config: &AppConfig) -> Self {
        let fallback = Language::from_str_opt(&config.i18n.language);
        Self(locale_from_header(headers.get(ACCEPT_LANGUAGE), fallback))
    }
}
impl FromRequestParts<crate::app::AppState> for Locale {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &crate::app::AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_parts(&parts.headers, &state.config))
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

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, StatusCode};
    use pichost_core::i18n::{I18n, Language};

    use super::{error_json, locale_from_header};

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
}
