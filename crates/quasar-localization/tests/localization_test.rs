//! Public API tests for quasar-localization.

use fluent::FluentValue;
use quasar_localization::formatter::plural_category_for_locale;
use quasar_localization::{
    AssetLoader, LoadError, Locale, LocaleDetector, LocaleFallback, Localization,
    LocalizationConfig, LocalizationError, MessageFormatter, PluralCategory, StringTable,
    TranslationSource,
};
use std::collections::HashMap;

#[test]
fn locale_creation_parse_and_fallback_chain() {
    let locale = Locale::new("en", Some("US"));
    assert_eq!(locale.to_string(), "en-US");
    assert_eq!(locale.language(), "en");
    assert_eq!(locale.region(), Some("US"));

    let parsed = Locale::parse("es-MX").expect("valid locale");
    assert_eq!(parsed, Locale::new("es", Some("MX")));

    let fallback = LocaleFallback::new(locale);
    assert_eq!(
        fallback.chain(),
        &[
            Locale::new("en", Some("US")),
            Locale::new("en", None),
            Locale::en_us()
        ]
    );
}

#[test]
fn string_table_stores_values_for_a_locale() {
    let mut table = StringTable::new(Locale::en_us());

    table.insert("hello", "Hello, World!");
    table.insert("goodbye", "Goodbye!");

    assert_eq!(table.locale(), &Locale::en_us());
    assert_eq!(table.get("hello"), Some("Hello, World!"));
    assert_eq!(table.get("missing"), None);
    assert_eq!(table.len(), 2);
    assert!(!table.is_empty());
}

#[test]
fn formatter_interpolates_fluent_values_and_plural_categories() {
    let formatter = MessageFormatter::new();
    let mut args = HashMap::new();
    args.insert("name".to_string(), FluentValue::String("World".into()));

    assert_eq!(formatter.format("Hello, {name}!", &args), "Hello, World!");
    assert_eq!(PluralCategory::from_str("one"), Some(PluralCategory::One));
    assert_eq!(PluralCategory::One.as_str(), "one");
    assert_eq!(plural_category_for_locale("en-US", 1), PluralCategory::One);
    assert_eq!(
        plural_category_for_locale("en-US", 2),
        PluralCategory::Other
    );
}

#[test]
fn asset_loader_loads_json_string_tables() {
    let loader = AssetLoader::new();
    let locale = Locale::en_us();
    let source = TranslationSource::Json(r#"{"hello":"Hello"}"#.to_string());

    let table = loader
        .load_string_table(&locale, source)
        .expect("json should parse")
        .expect("json source should produce a string table");

    assert_eq!(table.get("hello"), Some("Hello"));
}

#[test]
fn localization_loads_translations_and_switches_locale() {
    let localization = Localization::new(LocalizationConfig::default());
    localization
        .load_locale(
            Locale::en_us(),
            TranslationSource::Memory("greeting = Hello, {$name}!".to_string()),
        )
        .expect("locale should load");

    let mut args = HashMap::new();
    args.insert("name".to_string(), FluentValue::String("World".into()));

    assert_eq!(
        strip_fluent_isolates(&localization.tr_with_args("greeting", &args)),
        "Hello, World!"
    );
    assert_eq!(localization.tr("missing"), "{missing}");

    localization
        .load_locale(
            Locale::es(),
            TranslationSource::Json(r#"{"hello":"Hola"}"#.to_string()),
        )
        .expect("locale should load");
    localization
        .set_locale(Locale::es())
        .expect("locale exists");

    assert_eq!(localization.tr("hello"), "Hola");
    assert!(localization.available_locales().contains(&Locale::es()));
}

#[test]
fn localization_plural_passes_count_argument() {
    let localization = Localization::new(LocalizationConfig::default());
    localization
        .load_locale(
            Locale::en_us(),
            TranslationSource::Memory("item-count = You have {$count} items".to_string()),
        )
        .expect("locale should load");

    assert_eq!(
        strip_fluent_isolates(&localization.tr_plural("item-count", 5)),
        "You have 5 items"
    );
}

#[test]
fn locale_detector_constructs_and_detects_fallback() {
    let _detector = LocaleDetector::new();
    let detected = LocaleDetector::detect();

    assert!(!detected.language().is_empty());
}

#[test]
fn translation_source_variants_keep_their_payloads() {
    let path = std::path::PathBuf::from("test.ftl");
    let source = TranslationSource::File(path.clone());
    assert!(matches!(source, TranslationSource::File(p) if p == path));

    let source = TranslationSource::Toml(r#"hello = "world""#.to_string());
    assert!(matches!(source, TranslationSource::Toml(t) if t.contains("hello")));
}

#[test]
fn localization_errors_format_useful_messages() {
    let error = LocalizationError::LoadFailed(LoadError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "test",
    )));
    assert!(error.to_string().contains("Failed to load translation"));

    let error = LocalizationError::MessageNotFound {
        key: "test_key".to_string(),
        locale: "en-US".to_string(),
    };
    assert!(error.to_string().contains("test_key"));
    assert!(error.to_string().contains("en-US"));
}

fn strip_fluent_isolates(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
        .collect()
}
