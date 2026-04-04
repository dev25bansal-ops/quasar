use crate::locale::Locale;
use crate::localization::StringTable;
use crate::{LocalizationError, Result};
use fluent::FluentResource;
use fluent_bundle::FluentBundle;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub enum TranslationSource {
    File(PathBuf),
    Memory(String),
    Json(String),
    Toml(String),
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Resource not found: {0}")]
    NotFound(PathBuf),
}

pub struct AssetLoader;

impl AssetLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn load(
        &self,
        locale: &Locale,
        source: TranslationSource,
    ) -> Result<FluentBundle<FluentResource>> {
        match source {
            TranslationSource::File(path) => self.load_from_file(locale, &path),
            TranslationSource::Memory(content) => self.load_from_string(locale, &content),
            TranslationSource::Json(content) => self.load_from_json(locale, &content),
            TranslationSource::Toml(content) => self.load_from_toml(locale, &content),
        }
    }

    fn load_from_file(&self, locale: &Locale, path: &Path) -> Result<FluentBundle<FluentResource>> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| LocalizationError::LoadFailed(LoadError::Io(e)))?;

        let resource = FluentResource::try_new(content).map_err(|(_, errs)| {
            LocalizationError::Parse(format!("Fluent parse errors: {:?}", errs))
        })?;

        let lang_id = locale.to_langid();
        let mut bundle: FluentBundle<FluentResource> = FluentBundle::new(vec![lang_id]);
        bundle
            .add_resource(resource)
            .map_err(|errs| LocalizationError::Parse(format!("Bundle errors: {:?}", errs)))?;

        Ok(bundle)
    }

    fn load_from_string(
        &self,
        locale: &Locale,
        content: &str,
    ) -> Result<FluentBundle<FluentResource>> {
        let resource = FluentResource::try_new(content.to_string()).map_err(|(_, errs)| {
            LocalizationError::Parse(format!("Fluent parse errors: {:?}", errs))
        })?;

        let lang_id = locale.to_langid();
        let mut bundle: FluentBundle<FluentResource> = FluentBundle::new(vec![lang_id]);
        bundle
            .add_resource(resource)
            .map_err(|errs| LocalizationError::Parse(format!("Bundle errors: {:?}", errs)))?;

        Ok(bundle)
    }

    fn load_from_json(
        &self,
        locale: &Locale,
        content: &str,
    ) -> Result<FluentBundle<FluentResource>> {
        let json: HashMap<String, String> =
            serde_json::from_str(content).map_err(|e| LocalizationError::Parse(e.to_string()))?;

        let fluent_content = json
            .iter()
            .map(|(k, v)| format!("{k} = {v}"))
            .collect::<Vec<_>>()
            .join("\n");

        self.load_from_string(locale, &fluent_content)
    }

    fn load_from_toml(
        &self,
        locale: &Locale,
        content: &str,
    ) -> Result<FluentBundle<FluentResource>> {
        let toml_value: toml::Value =
            toml::from_str(content).map_err(|e| LocalizationError::Parse(e.to_string()))?;

        let fluent_content = self.toml_to_fluent(&toml_value, "");
        self.load_from_string(locale, &fluent_content)
    }

    fn toml_to_fluent(&self, value: &toml::Value, prefix: &str) -> String {
        match value {
            toml::Value::String(s) => format!("{} = {}", prefix, s),
            toml::Value::Table(table) => table
                .iter()
                .map(|(k, v)| {
                    let new_prefix = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    self.toml_to_fluent(v, &new_prefix)
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    }

    pub fn load_string_table(
        &self,
        locale: &Locale,
        source: TranslationSource,
    ) -> Result<Option<StringTable>> {
        match source {
            TranslationSource::Json(content) => {
                let json: HashMap<String, String> = serde_json::from_str(&content)
                    .map_err(|e| LocalizationError::Parse(e.to_string()))?;
                let mut table = StringTable::new(locale.clone());
                for (k, v) in json {
                    table.insert(k, v);
                }
                Ok(Some(table))
            }
            TranslationSource::File(path)
                if path.extension().map(|e| e == "json").unwrap_or(false) =>
            {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| LocalizationError::LoadFailed(LoadError::Io(e)))?;
                let json: HashMap<String, String> = serde_json::from_str(&content)
                    .map_err(|e| LocalizationError::Parse(e.to_string()))?;
                let mut table = StringTable::new(locale.clone());
                for (k, v) in json {
                    table.insert(k, v);
                }
                Ok(Some(table))
            }
            _ => Ok(None),
        }
    }

    pub fn discover_locales(&self, directory: &Path) -> Result<Vec<Locale>> {
        let mut locales = Vec::new();

        if !directory.exists() {
            return Ok(locales);
        }

        for entry in WalkDir::new(directory).min_depth(1).max_depth(2) {
            let entry = entry.map_err(|e| {
                LocalizationError::LoadFailed(LoadError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                )))
            })?;

            if let Some(name) = entry.file_name().to_str() {
                if let Some(locale) = Locale::parse(name) {
                    locales.push(locale);
                }
            }
        }

        locales.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        locales.dedup();

        Ok(locales)
    }
}

impl Default for AssetLoader {
    fn default() -> Self {
        Self::new()
    }
}
