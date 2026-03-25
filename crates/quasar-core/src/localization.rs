//! Localization System for Quasar Engine.
//!
//! Provides internationalization (i18n) support with:
//! - Multiple language support
//! - String interpolation with named parameters
//! - Pluralization rules
//! - Fallback language chain
//! - Runtime language switching
//!
//! # Example
//!
//! ```ignore
//! use quasar_core::localization::*;
//!
//! let mut loc = Localization::new();
//! loc.set_fallback_language("en");
//! loc.add_strings("en", json_strings);
//! loc.add_strings("es", json_strings_es);
//!
//! // Get a localized string
//! let greeting = loc.get("ui.greeting");
//!
//! // With interpolation
//! let msg = loc.get_with_args("ui.welcome", &[("name", "Player")]);
//!
//! // Pluralization
//! let items = loc.get_plural("ui.items", count);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

/// A language code (e.g., "en", "en-US", "es-ES").
pub type LanguageCode = String;

/// A localization key (e.g., "ui.greeting", "items.sword.name").
pub type LocKey = String;

/// A localized string value.
#[derive(Debug, Clone)]
pub struct LocalizedString {
    /// The key for this string.
    pub key: LocKey,
    /// The string value.
    pub value: String,
    /// Optional plural forms.
    pub plural_forms: Option<PluralForms>,
    /// Optional description/context for translators.
    pub description: Option<String>,
}

/// Plural forms for a string.
#[derive(Debug, Clone)]
pub struct PluralForms {
    /// Plural form for "one" (1 item).
    pub one: Option<String>,
    /// Plural form for "other" (0, 2+ items).
    pub other: String,
    /// Additional language-specific forms.
    pub additional: HashMap<String, String>,
}

impl PluralForms {
    pub fn new(other: String) -> Self {
        Self {
            one: None,
            other,
            additional: HashMap::new(),
        }
    }

    pub fn with_one(mut self, one: String) -> Self {
        self.one = Some(one);
        self
    }

    /// Get the appropriate plural form for a count.
    pub fn get(&self, count: i64, language: &str) -> &str {
        // Simple plural rules - can be extended for more complex languages
        let form = plural_category(count, language);

        match form {
            "one" => self.one.as_deref().unwrap_or(&self.other),
            _ => self.additional.get(form).unwrap_or(&self.other),
        }
    }
}

/// Determine the plural category for a count in a given language.
pub fn plural_category(count: i64, language: &str) -> &'static str {
    // Simplified plural rules based on CLDR
    // Full implementation would use the full CLDR plural rules

    let lang = language.split('-').next().unwrap_or(language);

    match lang {
        // Languages with only "other" form (Chinese, Japanese, Korean, etc.)
        "zh" | "ja" | "ko" | "vi" | "th" | "id" => "other",

        // Languages with "one" and "other" (most Indo-European)
        "en" | "de" | "nl" | "sv" | "no" | "da" | "es" | "pt" | "it" | "fr" => {
            if count == 1 {
                "one"
            } else {
                "other"
            }
        }

        // Russian, Polish, Ukrainian have complex plural rules
        "ru" | "uk" => {
            let mod10 = count % 10;
            let mod100 = count % 100;

            if mod10 == 1 && mod100 != 11 {
                "one"
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                "few"
            } else {
                "other"
            }
        }

        "pl" => {
            let mod10 = count % 10;
            let mod100 = count % 100;

            if count == 1 {
                "one"
            } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                "few"
            } else {
                "other"
            }
        }

        // Arabic has 6 plural forms
        "ar" => {
            let mod100 = count % 100;

            if count == 0 {
                "zero"
            } else if count == 1 {
                "one"
            } else if count == 2 {
                "two"
            } else if (3..=10).contains(&mod100) {
                "few"
            } else if (11..=99).contains(&mod100) {
                "many"
            } else {
                "other"
            }
        }

        // Default to simple "one"/"other"
        _ => {
            if count == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

/// String table for a single language.
#[derive(Debug, Clone, Default)]
pub struct StringTable {
    strings: HashMap<LocKey, LocalizedString>,
}

impl StringTable {
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
        }
    }

    pub fn add(&mut self, string: LocalizedString) {
        self.strings.insert(string.key.clone(), string);
    }

    pub fn get(&self, key: &str) -> Option<&LocalizedString> {
        self.strings.get(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.strings.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &LocKey> {
        self.strings.keys()
    }

    pub fn insert_simple(&mut self, key: &str, value: &str) {
        self.strings.insert(
            key.to_string(),
            LocalizedString {
                key: key.to_string(),
                value: value.to_string(),
                plural_forms: None,
                description: None,
            },
        );
    }
}

/// Localization system.
#[derive(Debug, Clone)]
pub struct Localization {
    /// Current language.
    current_language: LanguageCode,
    /// Fallback languages (tried in order if key not found).
    fallback_languages: Vec<LanguageCode>,
    /// String tables for each language.
    tables: HashMap<LanguageCode, StringTable>,
    /// Interpolation pattern (default: {name}).
    interpolation_pattern: String,
}

impl Localization {
    pub fn new() -> Self {
        Self {
            current_language: "en".to_string(),
            fallback_languages: vec!["en".to_string()],
            tables: HashMap::new(),
            interpolation_pattern: r"\{([^}]+)\}".to_string(),
        }
    }

    pub fn with_language(language: &str) -> Self {
        Self {
            current_language: language.to_string(),
            fallback_languages: vec![language.to_string()],
            tables: HashMap::new(),
            interpolation_pattern: r"\{([^}]+)\}".to_string(),
        }
    }

    /// Set the current language.
    pub fn set_language(&mut self, language: &str) {
        self.current_language = language.to_string();
    }

    /// Get the current language.
    pub fn language(&self) -> &str {
        &self.current_language
    }

    /// Set fallback languages (tried in order).
    pub fn set_fallback_languages(&mut self, languages: &[&str]) {
        self.fallback_languages = languages.iter().map(|s| s.to_string()).collect();
    }

    /// Add a fallback language.
    pub fn add_fallback_language(&mut self, language: &str) {
        if !self.fallback_languages.iter().any(|l| l == language) {
            self.fallback_languages.push(language.to_string());
        }
    }

    /// Add a string table for a language.
    pub fn add_table(&mut self, language: &str, table: StringTable) {
        self.tables.insert(language.to_string(), table);
    }

    /// Add a simple key-value pair.
    pub fn add_string(&mut self, language: &str, key: &str, value: &str) {
        let table = self.tables.entry(language.to_string()).or_default();
        table.insert_simple(key, value);
    }

    /// Add multiple strings from a map.
    pub fn add_strings(&mut self, language: &str, strings: HashMap<String, String>) {
        let table = self.tables.entry(language.to_string()).or_default();
        for (key, value) in strings {
            table.insert_simple(&key, &value);
        }
    }

    /// Add a localized string with plural forms.
    pub fn add_plural_string(&mut self, language: &str, key: &str, one: Option<&str>, other: &str) {
        let table = self.tables.entry(language.to_string()).or_default();
        let mut plural = PluralForms::new(other.to_string());
        if let Some(one_str) = one {
            plural = plural.with_one(one_str.to_string());
        }

        table.strings.insert(
            key.to_string(),
            LocalizedString {
                key: key.to_string(),
                value: other.to_string(),
                plural_forms: Some(plural),
                description: None,
            },
        );
    }

    /// Get a localized string.
    pub fn get(&self, key: &str) -> String {
        self.get_with_args(key, &[])
    }

    /// Get a localized string with interpolation.
    pub fn get_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        // Try current language first
        if let Some(table) = self.tables.get(&self.current_language) {
            if let Some(loc) = table.get(key) {
                return self.interpolate(&loc.value, args);
            }
        }

        // Try fallback languages
        for fallback in &self.fallback_languages {
            if fallback == &self.current_language {
                continue;
            }
            if let Some(table) = self.tables.get(fallback) {
                if let Some(loc) = table.get(key) {
                    return self.interpolate(&loc.value, args);
                }
            }
        }

        // Return key as fallback
        format!("[{}]", key)
    }

    /// Get a pluralized string.
    pub fn get_plural(&self, key: &str, count: i64) -> String {
        self.get_plural_with_args(key, count, &[])
    }

    /// Get a pluralized string with interpolation.
    pub fn get_plural_with_args(&self, key: &str, count: i64, args: &[(&str, &str)]) -> String {
        let count_str = count.to_string();

        // Try current language first
        if let Some(table) = self.tables.get(&self.current_language) {
            if let Some(loc) = table.get(key) {
                if let Some(ref plural) = loc.plural_forms {
                    let value = plural.get(count, &self.current_language);
                    let mut all_args: Vec<(&str, &str)> = args.to_vec();
                    all_args.push(("count", &count_str));
                    return self.interpolate(value, &all_args);
                }
            }
        }

        // Try fallback languages
        for fallback in &self.fallback_languages {
            if fallback == &self.current_language {
                continue;
            }
            if let Some(table) = self.tables.get(fallback) {
                if let Some(loc) = table.get(key) {
                    if let Some(ref plural) = loc.plural_forms {
                        let value = plural.get(count, fallback);
                        let mut all_args: Vec<(&str, &str)> = args.to_vec();
                        all_args.push(("count", &count_str));
                        return self.interpolate(value, &all_args);
                    }
                }
            }
        }

        // Fallback to key with count
        let mut all_args: Vec<(&str, &str)> = args.to_vec();
        all_args.push(("count", &count_str));
        self.interpolate(&format!("[{}]", key), &all_args)
    }

    /// Check if a key exists.
    pub fn has(&self, key: &str) -> bool {
        if let Some(table) = self.tables.get(&self.current_language) {
            if table.contains(key) {
                return true;
            }
        }

        for fallback in &self.fallback_languages {
            if let Some(table) = self.tables.get(fallback) {
                if table.contains(key) {
                    return true;
                }
            }
        }

        false
    }

    /// Get all keys for a language.
    pub fn keys(&self, language: &str) -> Vec<&LocKey> {
        self.tables
            .get(language)
            .map(|t| t.keys().collect())
            .unwrap_or_default()
    }

    /// Interpolate arguments into a string.
    fn interpolate(&self, template: &str, args: &[(&str, &str)]) -> String {
        let mut result = template.to_string();

        for (key, value) in args {
            let pattern = format!("{{{}}}", key);
            result = result.replace(&pattern, value);
        }

        result
    }

    /// Get the number of strings for a language.
    pub fn string_count(&self, language: &str) -> usize {
        self.tables.get(language).map(|t| t.len()).unwrap_or(0)
    }

    /// List available languages.
    pub fn available_languages(&self) -> Vec<&LanguageCode> {
        self.tables.keys().collect()
    }
}

impl Default for Localization {
    fn default() -> Self {
        Self::new()
    }
}

/// Localization resource for ECS.
#[derive(Debug, Clone)]
pub struct LocalizationResource {
    pub localization: Arc<std::sync::RwLock<Localization>>,
}

impl LocalizationResource {
    pub fn new(localization: Localization) -> Self {
        Self {
            localization: Arc::new(std::sync::RwLock::new(localization)),
        }
    }

    pub fn get(&self, key: &str) -> String {
        self.localization.read().unwrap().get(key)
    }

    pub fn get_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.localization.read().unwrap().get_with_args(key, args)
    }

    pub fn get_plural(&self, key: &str, count: i64) -> String {
        self.localization.read().unwrap().get_plural(key, count)
    }

    pub fn set_language(&self, language: &str) {
        self.localization.write().unwrap().set_language(language);
    }
}

/// Localization plugin.
pub struct LocalizationPlugin {
    pub default_language: String,
    pub fallback_languages: Vec<String>,
}

impl LocalizationPlugin {
    pub fn new() -> Self {
        Self {
            default_language: "en".to_string(),
            fallback_languages: vec!["en".to_string()],
        }
    }

    pub fn with_language(language: &str) -> Self {
        Self {
            default_language: language.to_string(),
            fallback_languages: vec![language.to_string()],
        }
    }
}

impl Default for LocalizationPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::Plugin for LocalizationPlugin {
    fn name(&self) -> &str {
        "LocalizationPlugin"
    }

    fn build(&self, app: &mut crate::App) {
        let mut loc = Localization::with_language(&self.default_language);
        for fallback in &self.fallback_languages {
            loc.add_fallback_language(fallback);
        }

        app.world.insert_resource(LocalizationResource::new(loc));
        log::info!("LocalizationPlugin loaded — i18n system active");
    }
}

/// Helper macro for getting localized strings.
#[macro_export]
macro_rules! loc {
    ($resource:expr, $key:expr) => {
        $resource.get($key)
    };
    ($resource:expr, $key:expr, $($arg_key:expr => $arg_val:expr),+ $(,)?) => {
        $resource.get_with_args($key, &[$(($arg_key, $arg_val)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localization_basic() {
        let mut loc = Localization::new();
        loc.add_string("en", "greeting", "Hello, World!");

        assert_eq!(loc.get("greeting"), "Hello, World!");
    }

    #[test]
    fn localization_fallback() {
        let mut loc = Localization::new();
        loc.set_language("es");
        loc.set_fallback_languages(&["en"]);
        loc.add_string("en", "greeting", "Hello, World!");
        loc.add_string("es", "farewell", "¡Adiós!");

        assert_eq!(loc.get("greeting"), "Hello, World!");
        assert_eq!(loc.get("farewell"), "¡Adiós!");
    }

    #[test]
    fn localization_interpolation() {
        let mut loc = Localization::new();
        loc.add_string("en", "welcome", "Welcome, {name}!");

        assert_eq!(
            loc.get_with_args("welcome", &[("name", "Player")]),
            "Welcome, Player!"
        );
    }

    #[test]
    fn localization_plural_english() {
        let mut loc = Localization::new();
        loc.add_plural_string("en", "items", Some("1 item"), "{count} items");

        assert_eq!(loc.get_plural("items", 1), "1 item");
        assert_eq!(loc.get_plural("items", 0), "0 items");
        assert_eq!(loc.get_plural("items", 5), "5 items");
    }

    #[test]
    fn localization_missing_key() {
        let loc = Localization::new();
        assert_eq!(loc.get("missing"), "[missing]");
    }

    #[test]
    fn plural_category_english() {
        assert_eq!(plural_category(1, "en"), "one");
        assert_eq!(plural_category(0, "en"), "other");
        assert_eq!(plural_category(2, "en"), "other");
        assert_eq!(plural_category(100, "en"), "other");
    }

    #[test]
    fn plural_category_russian() {
        assert_eq!(plural_category(1, "ru"), "one");
        assert_eq!(plural_category(2, "ru"), "few");
        assert_eq!(plural_category(5, "ru"), "other");
        assert_eq!(plural_category(11, "ru"), "other");
        assert_eq!(plural_category(21, "ru"), "one");
    }

    #[test]
    fn plural_category_arabic() {
        assert_eq!(plural_category(0, "ar"), "zero");
        assert_eq!(plural_category(1, "ar"), "one");
        assert_eq!(plural_category(2, "ar"), "two");
        assert_eq!(plural_category(5, "ar"), "few");
        assert_eq!(plural_category(50, "ar"), "many");
        assert_eq!(plural_category(200, "ar"), "other");
    }

    #[test]
    fn string_table_operations() {
        let mut table = StringTable::new();
        table.insert_simple("key1", "value1");
        table.insert_simple("key2", "value2");

        assert!(table.contains("key1"));
        assert_eq!(table.len(), 2);
        assert!(!table.is_empty());
    }

    #[test]
    fn localization_resource() {
        let mut loc = Localization::new();
        loc.add_string("en", "test", "Test String");

        let resource = LocalizationResource::new(loc);
        assert_eq!(resource.get("test"), "Test String");
    }

    #[test]
    fn localization_multiple_args() {
        let mut loc = Localization::new();
        loc.add_string(
            "en",
            "message",
            "{greeting}, {name}! You have {count} messages.",
        );

        let result = loc.get_with_args(
            "message",
            &[("greeting", "Hello"), ("name", "Player"), ("count", "5")],
        );

        assert_eq!(result, "Hello, Player! You have 5 messages.");
    }

    #[test]
    fn localization_available_languages() {
        let mut loc = Localization::new();
        loc.add_string("en", "test", "English");
        loc.add_string("es", "test", "Spanish");
        loc.add_string("fr", "test", "French");

        let mut langs: Vec<_> = loc
            .available_languages()
            .into_iter()
            .map(|s| s.as_str())
            .collect();
        langs.sort();

        assert_eq!(langs, vec!["en", "es", "fr"]);
    }
}
