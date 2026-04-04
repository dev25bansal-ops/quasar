use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use unic_langid::LanguageIdentifier;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Locale {
    langid: LanguageIdentifier,
}

impl Locale {
    pub fn new(language: &str, region: Option<&str>) -> Self {
        let locale_str = match region {
            Some(r) => format!("{}-{}", language, r),
            None => language.to_string(),
        };
        let langid: LanguageIdentifier = locale_str
            .parse()
            .unwrap_or_else(|_| "en-US".parse().unwrap());
        Self { langid }
    }

    pub fn parse(s: &str) -> Option<Self> {
        s.parse::<LanguageIdentifier>()
            .ok()
            .map(|langid| Self { langid })
    }

    pub fn en_us() -> Self {
        Self::new("en", Some("US"))
    }

    pub fn en_gb() -> Self {
        Self::new("en", Some("GB"))
    }

    pub fn es() -> Self {
        Self::new("es", None)
    }

    pub fn es_mx() -> Self {
        Self::new("es", Some("MX"))
    }

    pub fn fr() -> Self {
        Self::new("fr", None)
    }

    pub fn de() -> Self {
        Self::new("de", None)
    }

    pub fn ja() -> Self {
        Self::new("ja", None)
    }

    pub fn zh_cn() -> Self {
        Self::new("zh", Some("CN"))
    }

    pub fn zh_tw() -> Self {
        Self::new("zh", Some("TW"))
    }

    pub fn ko() -> Self {
        Self::new("ko", None)
    }

    pub fn ru() -> Self {
        Self::new("ru", None)
    }

    pub fn ar() -> Self {
        Self::new("ar", None)
    }

    pub fn language(&self) -> &str {
        self.langid.language.as_str()
    }

    pub fn region(&self) -> Option<&str> {
        self.langid.region.as_ref().map(|r| r.as_str())
    }

    pub fn script(&self) -> Option<&str> {
        self.langid.script.as_ref().map(|s| s.as_str())
    }

    pub fn to_langid(&self) -> LanguageIdentifier {
        self.langid.clone()
    }

    pub fn is_rtl(&self) -> bool {
        matches!(self.langid.language.as_str(), "ar" | "he" | "fa" | "ur")
    }

    pub fn display_name(&self) -> String {
        format!(
            "{}-{}",
            self.langid.language.as_str(),
            self.langid
                .region
                .as_ref()
                .map(|r| r.as_str())
                .unwrap_or("")
        )
    }

    pub fn base_locale(&self) -> Self {
        Self::new(self.langid.language.as_str(), None)
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.langid)
    }
}

impl std::str::FromStr for Locale {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<LanguageIdentifier>()
            .map(|langid| Self { langid })
            .map_err(|e| e.to_string())
    }
}

pub struct LocaleFallback {
    chain: VecDeque<Locale>,
}

impl LocaleFallback {
    pub fn new(base: Locale) -> Self {
        let mut chain = VecDeque::new();
        chain.push_back(base.clone());
        if base.region().is_some() {
            chain.push_back(base.base_locale());
        }
        chain.push_back(Locale::en_us());
        Self { chain }
    }

    pub fn with_chain(chain: Vec<Locale>) -> Self {
        Self {
            chain: chain.into(),
        }
    }

    pub fn next(&mut self) -> Option<Locale> {
        self.chain.pop_front()
    }

    pub fn current(&self) -> Option<&Locale> {
        self.chain.front()
    }

    pub fn chain(&self) -> &[Locale] {
        self.chain.as_slices().0
    }

    pub fn add_fallback(&mut self, locale: Locale) {
        if !self.chain.contains(&locale) {
            self.chain.push_back(locale);
        }
    }
}

pub struct LocaleDetector;

impl LocaleDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect() -> Locale {
        Self::detect_system_locale().unwrap_or_else(|| Locale::en_us())
    }

    #[cfg(target_os = "windows")]
    pub fn detect_system_locale() -> Option<Locale> {
        use winapi::um::winnls::GetUserDefaultUILanguage;
        let lang_id = unsafe { GetUserDefaultUILanguage() };

        let primary = lang_id & 0x00FF;
        let sub = (lang_id >> 8) & 0x00FF;

        let lang = match primary {
            0x09 => "en",
            0x0C => "fr",
            0x07 => "de",
            0x0A => "es",
            0x11 => "ja",
            0x04 => "zh",
            0x12 => "ko",
            0x19 => "ru",
            0x01 => "ar",
            _ => "en",
        };

        let region = match sub {
            0x01 => Some("US"),
            0x02 => Some("GB"),
            0x03 => Some("CA"),
            0x07 => Some("DE"),
            0x0C => Some("FR"),
            0x0A => Some("ES"),
            0x11 => Some("JP"),
            0x04 => Some("CN"),
            0x12 => Some("KR"),
            0x19 => Some("RU"),
            _ => None,
        };

        Some(Locale::new(lang, region))
    }

    #[cfg(target_os = "macos")]
    pub fn detect_system_locale() -> Option<Locale> {
        None
    }

    #[cfg(target_os = "linux")]
    pub fn detect_system_locale() -> Option<Locale> {
        locale_config::Locale::user_default()
            .tags()
            .next()
            .and_then(|tag| Locale::parse(&tag.to_string()))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    pub fn detect_system_locale() -> Option<Locale> {
        std::env::var("LANG").ok().and_then(|lang| {
            let lang = lang.split('.').next()?;
            Locale::parse(lang)
        })
    }

    pub fn available_locales() -> Vec<Locale> {
        vec![
            Locale::en_us(),
            Locale::en_gb(),
            Locale::es(),
            Locale::es_mx(),
            Locale::fr(),
            Locale::de(),
            Locale::ja(),
            Locale::zh_cn(),
            Locale::zh_tw(),
            Locale::ko(),
            Locale::ru(),
            Locale::ar(),
        ]
    }
}

impl Default for LocaleDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_creation() {
        let locale = Locale::new("en", Some("US"));
        assert_eq!(locale.language(), "en");
        assert_eq!(locale.region(), Some("US"));
    }

    #[test]
    fn test_locale_parse() {
        let locale = Locale::parse("en-US").unwrap();
        assert_eq!(locale.language(), "en");
        assert_eq!(locale.region(), Some("US"));
    }

    #[test]
    fn test_fallback_chain() {
        let fallback = LocaleFallback::new(Locale::new("en", Some("GB")));
        let chain = fallback.chain();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0], Locale::new("en", Some("GB")));
        assert_eq!(chain[1], Locale::new("en", None));
        assert_eq!(chain[2], Locale::en_us());
    }

    #[test]
    fn test_rtl_detection() {
        assert!(Locale::ar().is_rtl());
        assert!(!Locale::en_us().is_rtl());
    }
}
