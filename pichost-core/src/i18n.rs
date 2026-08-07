use std::collections::HashMap;
use std::path::PathBuf;

use tracing::warn;

const EMBEDDED_EN: &str = include_str!("i18n/locales/en/messages.toml");
const EMBEDDED_ZH: &str = include_str!("i18n/locales/zh-CN/messages.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    ZhCN,
}

impl Language {
    pub fn from_str_opt(s: &str) -> Language {
        match s.to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" => Language::ZhCN,
            "en" => Language::En,
            other => {
                warn!("unsupported language {other:?}, falling back to en");
                Language::En
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhCN => "zh-CN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct I18n {
    language: Language,
    messages: HashMap<Language, HashMap<String, String>>,
}

impl I18n {
    pub fn from_maps(
        language: Language,
        messages: HashMap<Language, HashMap<String, String>>,
    ) -> Self {
        Self { language, messages }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    fn lookup(&self, locale: Language, key: &str) -> Option<String> {
        self.messages.get(&locale)?.get(key).cloned()
    }

    pub fn t(&self, locale: Language, key: &str) -> String {
        self.lookup(locale, key)
            .or_else(|| self.lookup(Language::En, key))
            .unwrap_or_else(|| key.to_string())
    }

    pub fn t_args(&self, locale: Language, key: &str, args: &[String]) -> String {
        let mut msg = self.t(locale, key);
        for a in args {
            msg = msg.replacen("{}", a, 1);
        }
        msg
    }

    pub fn load(language: Language, locales_dir: Option<PathBuf>) -> I18n {
        let mut messages = HashMap::new();
        messages.insert(Language::En, parse_toml(EMBEDDED_EN));
        messages.insert(Language::ZhCN, parse_toml(EMBEDDED_ZH));
        if let Some(dir) = locales_dir {
            for lang in [Language::En, Language::ZhCN] {
                let path = dir.join(lang.as_str()).join("messages.toml");
                if let Ok(content) = std::fs::read_to_string(&path) {
                    messages.entry(lang).or_default().extend(parse_toml(&content));
                } else {
                    warn!("locale file missing: {:?}", path);
                }
            }
        }
        I18n::from_maps(language, messages)
    }
}

fn parse_toml(content: &str) -> HashMap<String, String> {
    toml::from_str(content).unwrap_or_else(|e| {
        warn!("invalid messages.toml: {e}");
        HashMap::new()
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{I18n, Language};

    fn maps() -> HashMap<Language, HashMap<String, String>> {
        let mut en = HashMap::new();
        en.insert("greet".into(), "hello".into());
        let mut zh = HashMap::new();
        zh.insert("greet".into(), "你好".into());
        HashMap::from([(Language::En, en), (Language::ZhCN, zh)])
    }

    #[test]
    fn t_resolves_per_locale() {
        let i18n = I18n::from_maps(Language::En, maps());
        assert_eq!(i18n.t(Language::ZhCN, "greet"), "你好");
        assert_eq!(i18n.t(Language::En, "greet"), "hello");
    }

    #[test]
    fn t_falls_back_zh_to_en_to_key() {
        let i18n = I18n::from_maps(Language::En, maps());
        assert_eq!(i18n.t(Language::ZhCN, "missing"), "missing");
        let mut en_only = HashMap::new();
        en_only.insert("only_en".into(), "e".into());
        let i18n = I18n::from_maps(Language::En, HashMap::from([(Language::En, en_only)]));
        assert_eq!(i18n.t(Language::ZhCN, "only_en"), "e");
    }

    #[test]
    fn t_args_replaces_placeholders() {
        let mut en = HashMap::new();
        en.insert("rl".into(), "rate limit exceeded, retry after {}s".into());
        let i18n = I18n::from_maps(Language::En, HashMap::from([(Language::En, en)]));
        assert_eq!(
            i18n.t_args(Language::En, "rl", &["42".into()]),
            "rate limit exceeded, retry after 42s"
        );
    }

    #[test]
    fn language_from_str() {
        assert_eq!(Language::from_str_opt("zh"), Language::ZhCN);
        assert_eq!(Language::from_str_opt("zh-CN"), Language::ZhCN);
        assert_eq!(Language::from_str_opt("en"), Language::En);
        assert_eq!(Language::from_str_opt("fr"), Language::En);
    }

    #[test]
    fn load_embedded_catalogs() {
        let i18n = I18n::load(Language::En, None);
        assert_eq!(
            i18n.t(Language::En, "validation.invalid_credentials"),
            "invalid username or password"
        );
        assert_eq!(
            i18n.t(Language::ZhCN, "validation.invalid_credentials"),
            "用户名或密码错误"
        );
    }

    #[test]
    fn load_with_locales_dir_overrides() {
        let dir = std::env::temp_dir().join("pichost-i18n-test");
        let zh_dir = dir.join("zh-CN");
        std::fs::create_dir_all(&zh_dir).unwrap();
        std::fs::write(
            zh_dir.join("messages.toml"),
            "\"validation.invalid_credentials\" = \"自定义中文\"",
        )
        .unwrap();
        let i18n = I18n::load(Language::ZhCN, Some(dir.clone()));
        assert_eq!(
            i18n.t(Language::ZhCN, "validation.invalid_credentials"),
            "自定义中文"
        );
        assert_eq!(
            i18n.t(Language::En, "validation.invalid_credentials"),
            "invalid username or password"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
