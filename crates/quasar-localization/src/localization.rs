use crate::{
    formatter::MessageFormatter, loader::AssetLoader, locale::Locale, LocalizationError, Result,
};
use fluent::types::FluentNumber;
use fluent::FluentArgs;
use fluent_bundle::FluentBundle;
use parking_lot::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LocalizationConfig {
    pub default_locale: Locale,
    pub fallback_chain: Vec<Locale>,
    pub hot_reload: bool,
    pub translations_path: std::path::PathBuf,
}

impl Default for LocalizationConfig {
    fn default() -> Self {
        Self {
            default_locale: Locale::en_us(),
            fallback_chain: vec![Locale::en_us()],
            hot_reload: false,
            translations_path: std::path::PathBuf::from("locales"),
        }
    }
}

#[derive(Clone)]
pub struct StringTable {
    entries: HashMap<String, String>,
    locale: Locale,
}

impl StringTable {
    pub fn new(locale: Locale) -> Self {
        Self {
            entries: HashMap::new(),
            locale,
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub struct Localization {
    config: LocalizationConfig,
    current_locale: RwLock<Locale>,
    bundles: RwLock<HashMap<Locale, FluentBundle<fluent::FluentResource>>>,
    string_tables: RwLock<HashMap<Locale, StringTable>>,
    formatter: MessageFormatter,
    loader: AssetLoader,
}

impl Localization {
    pub fn new(config: LocalizationConfig) -> Self {
        let current_locale = config.default_locale.clone();
        Self {
            config,
            current_locale: RwLock::new(current_locale),
            bundles: RwLock::new(HashMap::new()),
            string_tables: RwLock::new(HashMap::new()),
            formatter: MessageFormatter::new(),
            loader: AssetLoader::new(),
        }
    }

    pub fn current_locale(&self) -> Locale {
        self.current_locale.read().clone()
    }

    pub fn set_locale(&self, locale: Locale) -> Result<()> {
        if !self.is_locale_available(&locale) {
            return Err(LocalizationError::LocaleNotFound(locale.to_string()));
        }
        *self.current_locale.write() = locale;
        Ok(())
    }

    pub fn is_locale_available(&self, locale: &Locale) -> bool {
        self.bundles.read().contains_key(locale) || self.string_tables.read().contains_key(locale)
    }

    pub fn available_locales(&self) -> Vec<Locale> {
        let mut locales: Vec<_> = self.bundles.read().keys().cloned().collect();
        let string_table_locales: Vec<_> = self.string_tables.read().keys().cloned().collect();
        locales.extend(string_table_locales);
        locales.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        locales.dedup();
        locales
    }

    pub fn load_locale(
        &self,
        locale: Locale,
        source: crate::loader::TranslationSource,
    ) -> Result<()> {
        let bundle = self.loader.load(&locale, source.clone())?;
        self.bundles.write().insert(locale.clone(), bundle);

        if let Some(string_table) = self.loader.load_string_table(&locale, source)? {
            self.string_tables.write().insert(locale, string_table);
        }

        Ok(())
    }

    pub fn tr(&self, key: &str) -> String {
        self.tr_with_args(key, &HashMap::new())
    }

    pub fn tr_with_args(&self, key: &str, args: &HashMap<String, fluent::FluentValue>) -> String {
        let locale = self.current_locale.read();

        if let Some(bundle) = self.bundles.read().get(&*locale) {
            if let Some(msg) = self.format_message(bundle, key, args) {
                return msg;
            }
        }

        for fallback in &self.config.fallback_chain {
            if let Some(bundle) = self.bundles.read().get(fallback) {
                if let Some(msg) = self.format_message(bundle, key, args) {
                    return msg;
                }
            }
        }

        if let Some(table) = self.string_tables.read().get(&*locale) {
            if let Some(value) = table.get(key) {
                return value.to_string();
            }
        }

        format!("{{{key}}}")
    }

    fn format_message(
        &self,
        bundle: &FluentBundle<fluent::FluentResource>,
        key: &str,
        args: &HashMap<String, fluent::FluentValue>,
    ) -> Option<String> {
        let msg = bundle.get_message(key)?;
        let pattern = msg.value()?;
        let mut errors = vec![];

        let fluent_args: FluentArgs = args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

        let value = bundle.format_pattern(pattern, Some(&fluent_args), &mut errors);
        if errors.is_empty() {
            Some(value.to_string())
        } else {
            None
        }
    }

    pub fn tr_plural(&self, key: &str, count: usize) -> String {
        let mut args = HashMap::new();
        args.insert(
            "count".to_string(),
            fluent::FluentValue::Number(FluentNumber::from(count as f64)),
        );
        self.tr_with_args(key, &args)
    }

    pub fn tr_gender(&self, key: &str, gender: &str) -> String {
        let mut args = HashMap::new();
        args.insert(
            "gender".to_string(),
            fluent::FluentValue::String(gender.into()),
        );
        self.tr_with_args(key, &args)
    }

    pub fn reload(&self) -> Result<()> {
        self.bundles.write().clear();
        self.string_tables.write().clear();
        Ok(())
    }

    pub fn formatter(&self) -> &MessageFormatter {
        &self.formatter
    }
}
