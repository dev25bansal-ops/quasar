//! Console panel — scrollable log viewer.

use std::collections::VecDeque;

/// Level of a console message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// A single console log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
}

/// A ring-buffer log that feeds the editor console panel.
pub struct ConsoleLog {
    entries: VecDeque<LogEntry>,
    capacity: usize,
    pub auto_scroll: bool,
}

impl ConsoleLog {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: 512,
            auto_scroll: true,
        }
    }

    /// Push a message into the console log.
    pub fn push(&mut self, level: LogLevel, message: impl Into<String>) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry {
            level,
            message: message.into(),
        });
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Info, msg);
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Warn, msg);
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.push(LogLevel::Error, msg);
    }

    /// Add an info log message (alias for info()).
    pub fn log(&mut self, msg: impl Into<String>) {
        self.info(msg);
    }

    /// Returns the number of log entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no log entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns an iterator over log entries.
    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    /// Clear all log entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Render the console panel.
    pub fn panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("console")
            .default_height(180.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📝 Console");
                    if ui.button("Clear").clicked() {
                        self.clear();
                    }
                    ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .stick_to_bottom(self.auto_scroll)
                    .show(ui, |ui| {
                        for entry in &self.entries {
                            let color = match entry.level {
                                LogLevel::Info => egui::Color32::LIGHT_GRAY,
                                LogLevel::Warn => egui::Color32::YELLOW,
                                LogLevel::Error => egui::Color32::from_rgb(255, 100, 100),
                            };
                            let prefix = match entry.level {
                                LogLevel::Info => "[INFO]",
                                LogLevel::Warn => "[WARN]",
                                LogLevel::Error => "[ERR ]",
                            };
                            ui.colored_label(color, format!("{} {}", prefix, entry.message));
                        }
                    });
            });
    }
}

impl Default for ConsoleLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_log_capacity() {
        let mut log = ConsoleLog::new();
        for i in 0..600 {
            log.info(format!("msg {}", i));
        }
        assert_eq!(log.entries.len(), 512);
    }

    #[test]
    fn console_log_clear() {
        let mut log = ConsoleLog::new();
        log.info("hello");
        log.warn("world");
        assert_eq!(log.entries.len(), 2);
        log.clear();
        assert_eq!(log.entries.len(), 0);
    }
}
