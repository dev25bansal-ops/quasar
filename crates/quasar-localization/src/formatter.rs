use fluent::FluentValue;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "zero" => Some(Self::Zero),
            "one" => Some(Self::One),
            "two" => Some(Self::Two),
            "few" => Some(Self::Few),
            "many" => Some(Self::Many),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }
}

pub struct MessageFormatter;

impl MessageFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn format(&self, pattern: &str, args: &HashMap<String, FluentValue>) -> String {
        let mut result = pattern.to_string();
        for (key, value) in args {
            let placeholder = format!("{{{key}}}");
            let replacement = match value {
                FluentValue::String(s) => s.to_string(),
                FluentValue::Number(n) => n.as_string().to_string(),
                _ => "".to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }
        result
    }

    pub fn format_select(
        &self,
        selector: &str,
        variants: &HashMap<String, String>,
        default: &str,
    ) -> String {
        variants.get(selector).cloned().unwrap_or_else(|| {
            variants
                .get(default)
                .cloned()
                .unwrap_or_else(|| default.to_string())
        })
    }

    pub fn format_plural(
        &self,
        count: usize,
        variants: &HashMap<PluralCategory, String>,
        category: PluralCategory,
    ) -> String {
        variants
            .get(&category)
            .or_else(|| variants.get(&PluralCategory::Other))
            .map(|s: &String| s.replace("{count}", &count.to_string()))
            .unwrap_or_else(|| format!("{count}"))
    }

    pub fn format_gender(&self, gender: &str, variants: &HashMap<String, String>) -> String {
        variants
            .get(gender)
            .or_else(|| variants.get("other"))
            .cloned()
            .unwrap_or_default()
    }

    pub fn interpolate(&self, template: &str, values: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in values {
            result = result.replace(&format!("{{{key}}}"), value);
        }
        result
    }
}

impl Default for MessageFormatter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn plural_category_for_locale(locale: &str, count: usize) -> PluralCategory {
    let count = count as f64;

    if locale.starts_with("en") {
        if count == 1.0 {
            PluralCategory::One
        } else {
            PluralCategory::Other
        }
    } else if locale.starts_with("ru") {
        let mod10 = count % 10.0;
        let mod100 = count % 100.0;
        if mod10 == 1.0 && mod100 != 11.0 {
            PluralCategory::One
        } else if mod10 >= 2.0 && mod10 <= 4.0 && !(mod100 >= 12.0 && mod100 <= 14.0) {
            PluralCategory::Few
        } else {
            PluralCategory::Other
        }
    } else if locale.starts_with("ar") {
        if count == 0.0 {
            PluralCategory::Zero
        } else if count == 1.0 {
            PluralCategory::One
        } else if count == 2.0 {
            PluralCategory::Two
        } else if count >= 3.0 && count <= 10.0 {
            PluralCategory::Few
        } else if count >= 11.0 && count <= 99.0 {
            PluralCategory::Many
        } else {
            PluralCategory::Other
        }
    } else if locale.starts_with("zh") || locale.starts_with("ja") || locale.starts_with("ko") {
        PluralCategory::Other
    } else {
        if count == 1.0 {
            PluralCategory::One
        } else {
            PluralCategory::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format() {
        let formatter = MessageFormatter::new();
        let mut args = HashMap::new();
        args.insert("name".to_string(), FluentValue::String("World".into()));
        let result = formatter.format("Hello, {name}!", &args);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_plural_category() {
        assert_eq!(
            plural_category_for_locale("en-US", 0),
            PluralCategory::Other
        );
        assert_eq!(plural_category_for_locale("en-US", 1), PluralCategory::One);
        assert_eq!(
            plural_category_for_locale("en-US", 2),
            PluralCategory::Other
        );
    }
}
