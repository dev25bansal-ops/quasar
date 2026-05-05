//! Tests for quasar-localization crate

use quasar_localization::prelude::*;

#[test]
fn test_locale_creation() {
    let locale = Locale::new("en-US");
    assert_eq!(locale.to_string(), "en-US");
}

#[test]
fn test_locale_from_str() {
    let locale = Locale::parse("en-US").unwrap();
    assert_eq!(locale.to_string(), "en-US");
}

#[test]
fn test_locale_fallback() {
    let locale = Locale::new("en-US");
    let fallback = locale.fallback();
    
    // en-US should fallback to en
    assert_eq!(fallback.to_string(), "en");
}

#[test]
fn test_string_table_creation() {
    let mut table = StringTable::new(Locale::new("en-US"));
    
    table.insert("hello", "Hello, World!");
    table.insert("goodbye", "Goodbye!");
    
    assert_eq!(table.get("hello"), Some("Hello, World!"));
    assert_eq!(table.get("goodbye"), Some("Goodbye!"));
    assert_eq!(table.get("missing"), None);
}

#[test]
fn test_string_table_locale() {
    let table = StringTable::new(Locale::new("en-US"));
    assert_eq!(table.locale().to_string(), "en-US");
}

#[test]
fn test_localization_config() {
    let config = LocalizationConfig::new()
        .with_default_locale(Locale::new("en-US"))
        .with_fallback_enabled(true);
    
    assert_eq!(config.default_locale().to_string(), "en-US");
    assert!(config.fallback_enabled());
}

#[test]
fn test_localization_creation() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    assert_eq!( localization.current_locale().to_string(), "en-US");
}

#[test]
fn test_localization_set_locale() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    localization.set_locale(Locale::new("es-ES")).unwrap();
    assert_eq!(localization.current_locale().to_string(), "es-ES");
}

#[test]
fn test_message_formatter() {
    let formatter = MessageFormatter::new();
    
    let message = formatter.format("Hello, {name}!", &[("name", "World")]);
    assert_eq!(message, "Hello, World!");
}

#[test]
fn test_plural_category() {
    assert_eq!(PluralCategory::from_count(0), PluralCategory::Other);
    assert_eq!(PluralCategory::from_count(1), PluralCategory::One);
    assert_eq!(PluralCategory::from_count(2), PluralCategory::Other);
}

#[test]
fn test_asset_loader_creation() {
    let loader = AssetLoader::new();
    assert!(loader.is_some());
}

#[test]
fn test_translation_source_file() {
    let path = std::path::PathBuf::from("test.ftl");
    let source = TranslationSource::File(path);
    
    match source {
        TranslationSource::File(p) => assert_eq!(p, std::path::PathBuf::from("test.ftl")),
        _ => panic!("Expected File variant"),
    }
}

#[test]
fn test_translation_source_memory() {
    let content = "test content".to_string();
    let source = TranslationSource::Memory(content.clone());
    
    match source {
        TranslationSource::Memory(c) => assert_eq!(c, content),
        _ => panic!("Expected Memory variant"),
    }
}

#[test]
fn test_translation_source_json() {
    let json = r#"{"key": "value"}"#.to_string();
    let source = TranslationSource::Json(json);
    
    match source {
        TranslationSource::Json(j) => assert_eq!(j, json),
        _ => panic!("Expected Json variant"),
    }
}

#[test]
fn test_translation_source_toml() {
    let toml = r#"key = "value""#.to_string();
    let source = TranslationSource::Toml(toml);
    
    match source {
        TranslationSource::Toml(t) => assert_eq!(t, toml),
        _ => panic!("Expected Toml variant"),
    }
}

#[test]
fn test_localization_error_load_failed() {
    let error = LocalizationError::LoadFailed(LoadError::Io(
        std::io::Error::new(std::io::ErrorKind::NotFound, "test")
    ));
    
    assert!(error.to_string().contains("Failed to load translation"));
}

#[test]
fn test_localization_error_locale_not_found() {
    let error = LocalizationError::LocaleNotFound("test-locale".to_string());
    assert_eq!(error.to_string(), "Locale not found: test-locale");
}

#[test]
fn test_localization_error_message_not_found() {
    let error = LocalizationError::MessageNotFound {
        key: "test_key".to_string(),
        locale: "en-US".to_string(),
    };
    
    assert!(error.to_string().contains("test_key"));
    assert!(error.to_string().contains("en-US"));
}

#[test]
fn test_string_table_update() {
    let mut table = StringTable::new(Locale::new("en-US"));
    
    table.insert("key1", "value1");
    table.insert("key1", "value2"); // Update
    
    assert_eq!(table.get("key1"), Some("value2"));
}

#[test]
fn test_string_table_remove() {
    let mut table = StringTable::new(Locale::new("en-US"));
    
    table.insert("temp_key", "temp_value");
    assert_eq!(table.get("temp_key"), Some("temp_value"));
    
    table.remove("temp_key");
    assert_eq!(table.get("temp_key"), None);
}

#[test]
fn test_string_table_keys() {
    let mut table = StringTable::new(Locale::new("en-US"));
    
    table.insert("key1", "value1");
    table.insert("key2", "value2");
    
    let keys: Vec<_> = table.keys().collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"key1"));
    assert!(keys.contains(&"key2"));
}

#[test]
fn test_string_table_len() {
    let mut table = StringTable::new(Locale::new("en-US"));
    
    assert_eq!(table.len(), 0);
    
    table.insert("key1", "value1");
    assert_eq!(table.len(), 1);
    
    table.insert("key2", "value2");
    assert_eq!(table.len(), 2);
}

#[test]
fn test_string_table_is_empty() {
    let table = StringTable::new(Locale::new("en-US"));
    assert!(table.is_empty());
    
    let mut table = StringTable::new(Locale::new("en-US"));
    table.insert("key", "value");
    assert!(!table.is_empty());
}

#[test]
fn test_locale_detector() {
    let detector = LocaleDetector::new();
    
    // Test that detector can be created
    assert!(detector.is_some());
}

#[test]
fn test_locale_detector_from_system() {
    let detector = LocaleDetector::new();
    let system_locale = detector.and_then(|d| d.detect_from_system());
    
    // System locale should be detected (or return None if not available)
    // Either way, the detector should work
    assert!(detector.is_some());
}

#[test]
fn test_localization_add_string_table() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    let mut table = StringTable::new(Locale::new("en-US"));
    table.insert("test_key", "test_value");
    
    localization.add_string_table(table).unwrap();
    
    // The table should be added
    assert!(localization.has_locale(&Locale::new("en-US")));
}

#[test]
fn test_localization_has_locale() {
    let localization = Localization::new(LocalizationConfig::new());
    
    assert!(localization.has_locale(&Locale::new("en-US")));
    assert!(!localization.has_locale(&Locale::new("xx-XX")));
}

#[test]
fn test_localization_available_locales() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    let mut table = StringTable::new(Locale::new("es-ES"));
    table.insert("key", "value");
    localization.add_string_table(table).unwrap();
    
    let locales = localization.available_locales();
    assert!(locales.contains(&Locale::new("en-US")));
    assert!(locales.contains(&Locale::new("es-ES")));
}

#[test]
fn test_localization_get() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    let mut table = StringTable::new(Locale::new("en-US"));
    table.insert("test_key", "test_value");
    localization.add_string_table(table).unwrap();
    
    assert_eq!(localization.get("test_key"), Some("test_value"));
    assert_eq!(localization.get("missing_key"), None);
}

#[test]
fn test_localization_get_with_args() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    let mut table = StringTable::new(Locale::new("en-US"));
    table.insert("greeting", "Hello, {name}!");
    localization.add_string_table(table).unwrap();
    
    let result = localization.get_with_args("greeting", &[("name", "World")]);
    assert_eq!(result, Some("Hello, World!".to_string()));
}

#[test]
fn test_localization_get_plural() {
    let mut localization = Localization::new(LocalizationConfig::new());
    
    let mut table = StringTable::new(Locale::new("en-US"));
    table.insert("item_count", "You have {count} item(s)");
    localization.add_string_table(table).unwrap();
    
    let result = localization.get_plural("item_count", 5, &[("count", "5")]);
    assert!(result.is_some());
}