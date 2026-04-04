pub mod formatter;
pub mod loader;
pub mod locale;
pub mod localization;

pub use formatter::{MessageFormatter, PluralCategory};
pub use loader::{AssetLoader, LoadError, TranslationSource};
pub use locale::{Locale, LocaleDetector, LocaleFallback};
pub use localization::{Localization, LocalizationConfig, StringTable};

pub type Result<T> = std::result::Result<T, LocalizationError>;

#[derive(Debug, thiserror::Error)]
pub enum LocalizationError {
    #[error("Failed to load translation: {0}")]
    LoadFailed(#[source] loader::LoadError),
    
    #[error("Locale not found: {0}")]
    LocaleNotFound(String),
    
    #[error("Message not found: {key} in locale {locale}")]
    MessageNotFound { key: String, locale: String },
    
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
    
    #[error("Fluent error: {0}")]
    FluentError(#[from] fluent::FluentError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    Parse(String),
}
