//! Quest journal UI component.
//!
//! Provides:
//! - Quest journal panel with active/completed/failed tabs
//! - Objective tracker mini-widget
//! - Dialogue panel for NPC conversations
//! - Reward notification popup
//!
//! Built with egui for integration with the Quasar editor/runtime.

use egui::{self, Color32, RichText, Ui, WidgetText};
use crate::widget::WidgetId;
use quasar_core::quest::{
    QuestCategory, QuestJournalEntry, QuestJournalFilter, QuestJournalSort,
    QuestObjectiveEntry, QuestState,
};

// ---------------------------------------------------------------------------
// Quest Journal Panel
// ---------------------------------------------------------------------------

/// State for the quest journal UI panel.
pub struct QuestJournalPanel {
    /// Whether the journal is open.
    pub open: bool,
    /// Current filter.
    pub filter: QuestJournalFilter,
    /// Current sort order.
    pub sort: QuestJournalSort,
    /// Selected quest ID for detail view.
    pub selected_quest: Option<String>,
    /// Active tab.
    pub tab: JournalTab,
    /// Search query.
    pub search_query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JournalTab {
    #[default]
    Active,
    Completed,
    Failed,
}

impl Default for QuestJournalPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl QuestJournalPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            filter: QuestJournalFilter::All,
            sort: QuestJournalSort::ByCategory,
            selected_quest: None,
            tab: JournalTab::Active,
            search_query: String::new(),
        }
    }

    /// Toggle the journal open/closed.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Show the quest journal window.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        entries: &[QuestJournalEntry],
        on_select_quest: impl Fn(&str),
        on_abandon_quest: impl Fn(&str),
    ) {
        if !self.open {
            return;
        }

        let mut open = true;
        egui::Window::new("Quest Journal")
            .open(&mut open)
            .default_width(500.0)
            .default_height(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                self.ui(ui, entries, &on_select_quest, &on_abandon_quest);
            });

        self.open = open;
    }

    /// Render the journal UI inside a container.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        entries: &[QuestJournalEntry],
        on_select_quest: &impl Fn(&str),
        on_abandon_quest: &impl Fn(&str),
    ) {
        // Tab bar
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, JournalTab::Active, "Active");
            ui.selectable_value(&mut self.tab, JournalTab::Completed, "Completed");
            ui.selectable_value(&mut self.tab, JournalTab::Failed, "Failed");

            ui.separator();

            // Search
            ui.text_edit_singleline(&mut self.search_query);

            ui.separator();

            // Sort dropdown
            if ui.button("Sort ▼").clicked() {
                // Cycle sort options
                self.sort = match self.sort {
                    QuestJournalSort::ByCategory => QuestJournalSort::ByState,
                    QuestJournalSort::ByState => QuestJournalSort::ByProgress,
                    QuestJournalSort::ByProgress => QuestJournalSort::ByStartTime,
                    QuestJournalSort::ByStartTime => QuestJournalSort::Alphabetical,
                    QuestJournalSort::Alphabetical => QuestJournalSort::ByCategory,
                };
            }
            ui.label(format!("{:?}", self.sort));
        });

        ui.separator();

        // Set filter based on tab
        self.filter = match self.tab {
            JournalTab::Active => QuestJournalFilter::Active,
            JournalTab::Completed => QuestJournalFilter::Completed,
            JournalTab::Failed => QuestJournalFilter::Failed,
        };

        // Filter by search query
        let filtered: Vec<&QuestJournalEntry> = entries
            .iter()
            .filter(|e| {
                if self.search_query.is_empty() {
                    return true;
                }
                e.title_key
                    .to_lowercase()
                    .contains(&self.search_query.to_lowercase())
                    || e.quest_id
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
            })
            .collect();

        // Quest list
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &filtered {
                let is_selected = self.selected_quest.as_deref() == Some(&entry.quest_id);

                ui.horizontal(|ui| {
                    // Category icon
                    let category_icon = match entry.category {
                        QuestCategory::Main => "📖",
                        QuestCategory::Side => "📋",
                        QuestCategory::Faction => "⚔️",
                        QuestCategory::Repeatable => "🔄",
                        QuestCategory::Hidden => "❓",
                        QuestCategory::Event => "🎉",
                        QuestCategory::Daily => "📅",
                        QuestCategory::Weekly => "📆",
                    };
                    ui.label(RichText::new(category_icon).size(16.0));

                    // State indicator
                    let state_color = match entry.state {
                        QuestState::Active => Color32::YELLOW,
                        QuestState::ReadyToTurnIn => Color32::GREEN,
                        QuestState::Completed => Color32::GRAY,
                        QuestState::Failed => Color32::RED,
                        QuestState::Available => Color32::BLUE,
                        QuestState::Locked => Color32::DARK_GRAY,
                    };
                    ui.label(
                        RichText::new("●")
                            .color(state_color)
                            .size(12.0),
                    );

                    // Title
                    let title_text = if entry.title_key.is_empty() {
                        &entry.quest_id
                    } else {
                        &entry.title_key
                    };
                    if ui
                        .selectable_label(is_selected, RichText::new(title_text))
                        .clicked()
                    {
                        self.selected_quest = Some(entry.quest_id.clone());
                        on_select_quest(&entry.quest_id);
                    }

                    // Progress
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{:.0}%", entry.progress * 100.0));

                        // Time remaining for timed quests
                        if let Some(time_str) = entry.format_time_remaining() {
                            ui.label(RichText::new(time_str).color(Color32::LIGHT_BLUE));
                        }
                    });
                });
            }

            if filtered.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No quests found");
                });
            }
        });

        // Selected quest details
        if let Some(ref quest_id) = self.selected_quest {
            if let Some(entry) = entries.iter().find(|e| e.quest_id == *quest_id) {
                ui.separator();
                self.show_quest_detail(ui, entry, on_abandon_quest);
            }
        }
    }

    fn show_quest_detail(
        &mut self,
        ui: &mut Ui,
        entry: &QuestJournalEntry,
        on_abandon_quest: &impl Fn(&str),
    ) {
        ui.vertical(|ui| {
            // Title
            let title = if entry.title_key.is_empty() {
                &entry.quest_id
            } else {
                &entry.title_key
            };
            ui.heading(RichText::new(title).size(16.0));

            // Description
            if !entry.description_key.is_empty() {
                ui.label(&entry.description_key);
            }

            // Category badge
            let category_text = format!("{:?}", entry.category);
            ui.label(RichText::new(category_text).size(10.0).color(Color32::GRAY));

            // Tags
            if !entry.tags.is_empty() {
                ui.horizontal(|ui| {
                    for tag in &entry.tags {
                        ui.label(
                            RichText::new(format!("#{}", tag))
                                .size(10.0)
                                .color(Color32::LIGHT_BLUE),
                        );
                    }
                });
            }

            ui.separator();

            // Objectives
            ui.label(RichText::new("Objectives:").strong());
            for objective in &entry.objectives {
                let status_icon = if objective.is_complete {
                    "✅"
                } else if objective.is_optional {
                    "⬜"
                } else {
                    "🔲"
                };

                ui.horizontal(|ui| {
                    ui.label(status_icon);

                    let objective_text = if objective.description_key.is_empty() {
                        &objective.id
                    } else {
                        &objective.description_key
                    };

                    let text_color = if objective.is_complete {
                        Color32::GREEN
                    } else {
                        Color32::WHITE
                    };
                    ui.label(RichText::new(objective_text).color(text_color));

                    // Progress
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!(
                            "{}/{}",
                            objective.progress, objective.required
                        ));
                    });
                });

                // Progress bar
                let progress_frac = if objective.required > 0 {
                    objective.progress as f32 / objective.required as f32
                } else {
                    1.0
                };
                let bar_width = ui.available_width() - 30.0;
                let progress_bar = egui::widgets::ProgressBar::new(progress_frac)
                    .desired_width(bar_width)
                    .show_percentage();
                ui.add(progress_bar);
            }

            // Abandon button (only for active quests)
            if entry.state == QuestState::Active
                || entry.state == QuestState::ReadyToTurnIn
            {
                ui.separator();
                if ui
                    .button(RichText::new("Abandon Quest").color(Color32::RED))
                    .clicked()
                {
                    on_abandon_quest(&entry.quest_id);
                    self.selected_quest = None;
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Objective Tracker (Mini Widget)
// ---------------------------------------------------------------------------

/// Mini widget showing tracked objectives on screen.
pub struct ObjectiveTrackerWidget {
    /// Maximum objectives to show.
    pub max_display: usize,
    /// Show progress bars.
    pub show_progress_bars: bool,
    /// Minimized state.
    pub minimized: bool,
    /// Objectives to track (quest_id, objective_id).
    pub tracked_objectives: Vec<(String, String)>,
}

impl Default for ObjectiveTrackerWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectiveTrackerWidget {
    pub fn new() -> Self {
        Self {
            max_display: 5,
            show_progress_bars: true,
            minimized: false,
            tracked_objectives: Vec::new(),
        }
    }

    /// Show the objective tracker panel.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        entries: &[QuestJournalEntry],
    ) {
        let mut open = true;
        egui::Window::new("Objectives")
            .open(&mut open)
            .default_width(300.0)
            .default_height(200.0)
            .resizable(false)
            .fixed_pos(egui::pos2(10.0, 50.0))
            .title_bar(!self.minimized)
            .show(ctx, |ui| {
                self.ui(ui, entries);
            });
    }

    /// Render inside a container.
    pub fn ui(&self, ui: &mut Ui, entries: &[QuestJournalEntry]) {
        if self.minimized {
            if ui.button("📋").clicked() {
                // Would toggle minimized state from outside
            }
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut shown = 0;
            for entry in entries {
                if shown >= self.max_display {
                    break;
                }

                if entry.state != QuestState::Active
                    && entry.state != QuestState::ReadyToTurnIn
                {
                    continue;
                }

                // Quest title
                let title = if entry.title_key.is_empty() {
                    &entry.quest_id
                } else {
                    &entry.title_key
                };
                ui.label(RichText::new(title).strong().size(12.0));

                // Objectives
                for objective in &entry.objectives {
                    if shown >= self.max_display {
                        break;
                    }
                    if objective.is_complete {
                        continue; // Hide completed objectives
                    }

                    let objective_text = if objective.description_key.is_empty() {
                        &objective.id
                    } else {
                        &objective.description_key
                    };

                    ui.horizontal(|ui| {
                        ui.label(format!("• {}", objective_text));
                        ui.label(format!(
                            "({}/{})",
                            objective.progress, objective.required
                        ));
                    });

                    if self.show_progress_bars {
                        let progress_frac = if objective.required > 0 {
                            objective.progress as f32 / objective.required as f32
                        } else {
                            1.0
                        };
                        let bar_width = ui.available_width() - 20.0;
                        let progress_bar = egui::widgets::ProgressBar::new(progress_frac)
                            .desired_width(bar_width)
                            .fill(Color32::from_rgb(100, 149, 237));
                        ui.add(progress_bar);
                    }

                    shown += 1;
                }

                if shown > 0 {
                    ui.separator();
                }
            }

            if shown == 0 {
                ui.label("No active objectives");
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Dialogue Panel
// ---------------------------------------------------------------------------

/// Dialogue panel for NPC conversations.
pub struct DialoguePanel {
    /// Whether the dialogue panel is visible.
    pub visible: bool,
    /// Current speaker name.
    pub speaker_name: String,
    /// Current dialogue text.
    pub dialogue_text: String,
    /// Available choices.
    pub choices: Vec<DialogueChoiceUI>,
    /// Selected choice index.
    pub selected_choice: usize,
    /// Typewriter effect active.
    pub typewriter_active: bool,
    /// Typewriter character index.
    pub typewriter_index: usize,
    /// Show portrait.
    pub show_portrait: bool,
    /// Portrait path.
    pub portrait_path: String,
}

#[derive(Debug, Clone)]
pub struct DialogueChoiceUI {
    pub text: String,
    pub index: usize,
    pub is_available: bool,
}

impl Default for DialoguePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl DialoguePanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            speaker_name: String::new(),
            dialogue_text: String::new(),
            choices: Vec::new(),
            selected_choice: 0,
            typewriter_active: false,
            typewriter_index: 0,
            show_portrait: true,
            portrait_path: String::new(),
        }
    }

    /// Show the dialogue panel.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        on_select_choice: impl Fn(usize),
        on_advance: impl Fn(),
    ) {
        if !self.visible {
            return;
        }

        let mut open = true;
        egui::Window::new("Dialogue")
            .open(&mut open)
            .default_width(600.0)
            .default_height(300.0)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -20.0])
            .show(ctx, |ui| {
                self.ui(ui, &on_select_choice, &on_advance);
            });

        // Don't close via X button
        self.visible = open && self.visible;
    }

    /// Render inside a container.
    pub fn ui(
        &self,
        ui: &mut Ui,
        on_select_choice: &impl Fn(usize),
        on_advance: &impl Fn(),
    ) {
        ui.vertical(|ui| {
            // Speaker name
            ui.horizontal(|ui| {
                if self.show_portrait && !self.portrait_path.is_empty() {
                    // Portrait placeholder
                    ui.label(RichText::new("👤").size(32.0));
                }
                ui.label(RichText::new(&self.speaker_name).strong().size(14.0));
            });

            ui.separator();

            // Dialogue text (with optional typewriter effect)
            let display_text = if self.typewriter_active {
                let chars: Vec<char> = self.dialogue_text.chars().collect();
                let visible_len = self.typewriter_index.min(chars.len());
                chars[..visible_len].iter().collect::<String>()
            } else {
                self.dialogue_text.clone()
            };
            ui.label(&display_text);

            // Click to advance if typewriter active or no choices
            if self.typewriter_active {
                if ui.button("Click to continue").clicked() {
                    on_advance();
                }
            }

            // Choices
            if !self.choices.is_empty() && !self.typewriter_active {
                ui.separator();
                ui.vertical(|ui| {
                    for (i, choice) in self.choices.iter().enumerate() {
                        let button_text = if choice.is_available {
                            &choice.text
                        } else {
                            "🔒 [Locked]"
                        };

                        let response = ui.button(button_text);
                        if choice.is_available && response.clicked() {
                            on_select_choice(i);
                        }
                    }
                });
            }
        });
    }

    /// Set the current dialogue line.
    pub fn set_dialogue(
        &mut self,
        speaker: impl Into<String>,
        text: impl Into<String>,
        choices: Vec<DialogueChoiceUI>,
    ) {
        self.speaker_name = speaker.into();
        self.dialogue_text = text.into();
        self.choices = choices;
        self.selected_choice = 0;
        self.typewriter_index = 0;
        self.typewriter_active = true;
    }

    /// Advance the typewriter effect.
    pub fn advance_typewriter(&mut self, chars_per_frame: usize) -> bool {
        if !self.typewriter_active {
            return false;
        }

        let total_chars = self.dialogue_text.chars().count();
        self.typewriter_index += chars_per_frame;

        if self.typewriter_index >= total_chars {
            self.typewriter_index = total_chars;
            self.typewriter_active = false;
            false
        } else {
            true
        }
    }

    /// Skip typewriter effect.
    pub fn skip_typewriter(&mut self) {
        self.typewriter_index = self.dialogue_text.chars().count();
        self.typewriter_active = false;
    }

    /// Show the dialogue.
    pub fn show_dialogue(&mut self) {
        self.visible = true;
    }

    /// Hide the dialogue.
    pub fn hide_dialogue(&mut self) {
        self.visible = false;
        self.choices.clear();
    }
}

// ---------------------------------------------------------------------------
// Reward Notification Popup
// ---------------------------------------------------------------------------

/// Reward notification popup.
pub struct RewardNotificationPopup {
    /// Active notifications.
    pub notifications: VecDeque<RewardNotification>,
    /// Maximum visible notifications.
    pub max_visible: usize,
}

#[derive(Debug, Clone)]
pub struct RewardNotification {
    pub message: String,
    pub reward_type: RewardType,
    pub icon: String,
    pub lifetime: f32,
    pub age: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardType {
    Experience,
    Gold,
    Item,
    Reputation,
    Achievement,
    QuestComplete,
    LevelUp,
    Unlock,
}

use std::collections::VecDeque;

impl Default for RewardNotificationPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl RewardNotificationPopup {
    pub fn new() -> Self {
        Self {
            notifications: VecDeque::with_capacity(8),
            max_visible: 5,
        }
    }

    /// Add a notification.
    pub fn add_notification(
        &mut self,
        message: impl Into<String>,
        reward_type: RewardType,
        icon: impl Into<String>,
    ) {
        let icon = icon.into();
        self.notifications.push_back(RewardNotification {
            message: message.into(),
            reward_type,
            icon: if icon.is_empty() {
                Self::default_icon_for_type(&reward_type).to_string()
            } else {
                icon
            },
            lifetime: 4.0,
            age: 0.0,
        });

        // Trim old notifications
        while self.notifications.len() > self.max_visible {
            self.notifications.pop_front();
        }
    }

    fn default_icon_for_type(reward_type: &RewardType) -> &'static str {
        match reward_type {
            RewardType::Experience => "⭐",
            RewardType::Gold => "💰",
            RewardType::Item => "🎁",
            RewardType::Reputation => "🤝",
            RewardType::Achievement => "🏆",
            RewardType::QuestComplete => "✅",
            RewardType::LevelUp => "⬆️",
            RewardType::Unlock => "🔓",
        }
    }

    /// Show notifications.
    pub fn show(&mut self, ctx: &egui::Context) {
        if self.notifications.is_empty() {
            return;
        }

        let screen_size = ctx.input(|i| i.screen_rect.size());
        egui::Area::new("reward_notifications".into())
            .fixed_pos(egui::pos2(screen_size.x - 320.0, 60.0))
            .show(ctx, |ui| {
                self.ui(ui);
            });
    }

    /// Render inside a container.
    pub fn ui(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            let mut to_remove: Vec<WidgetId> = Vec::new();
            for (i, notification) in self.notifications.iter().enumerate() {
                // Fade out near end of lifetime
                let remaining = notification.lifetime - notification.age;
                let alpha = if remaining < 1.0 {
                    remaining
                } else {
                    1.0
                };

                let bg_color = match notification.reward_type {
                    RewardType::Experience => Color32::from_rgba_premultiplied(255, 215, 0, (alpha * 200.0) as u8),
                    RewardType::Gold => Color32::from_rgba_premultiplied(255, 215, 0, (alpha * 200.0) as u8),
                    RewardType::Item => Color32::from_rgba_premultiplied(100, 149, 237, (alpha * 200.0) as u8),
                    RewardType::Reputation => Color32::from_rgba_premultiplied(144, 238, 144, (alpha * 200.0) as u8),
                    RewardType::Achievement => Color32::from_rgba_premultiplied(255, 165, 0, (alpha * 200.0) as u8),
                    RewardType::QuestComplete => Color32::from_rgba_premultiplied(50, 205, 50, (alpha * 200.0) as u8),
                    RewardType::LevelUp => Color32::from_rgba_premultiplied(255, 105, 180, (alpha * 200.0) as u8),
                    RewardType::Unlock => Color32::from_rgba_premultiplied(0, 255, 255, (alpha * 200.0) as u8),
                };

                egui::Frame::new()
                    .fill(bg_color)
                    .corner_radius(5)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&notification.icon).size(20.0));
                            ui.label(
                                RichText::new(&notification.message)
                                    .color(Color32::WHITE)
                                    .size(12.0),
                            );
                        });
                    });
            }
        });
    }

    /// Update notification ages.
    pub fn update(&mut self, dt: f32) {
        for notification in &mut self.notifications {
            notification.age += dt;
        }

        // Remove expired notifications
        self.notifications.retain(|n| n.age < n.lifetime);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use quasar_core::quest::{QuestJournalEntry, QuestObjectiveEntry, QuestState};

    fn create_test_entry(id: &str, state: QuestState) -> QuestJournalEntry {
        QuestJournalEntry {
            quest_id: id.to_string(),
            title_key: format!("quest.{}.title", id),
            description_key: format!("quest.{}.desc", id),
            category: QuestCategory::Main,
            state,
            objectives: vec![
                QuestObjectiveEntry {
                    id: "obj1".to_string(),
                    description_key: "objective 1".to_string(),
                    progress: 3,
                    required: 5,
                    is_complete: false,
                    is_optional: false,
                    objective_type: "DefeatEnemy".to_string(),
                },
            ],
            progress: 0.6,
            start_time: None,
            time_remaining: Some(120.0),
            icon: None,
            tags: vec!["story".to_string()],
        }
    }

    #[test]
    fn quest_journal_panel_creation() {
        let panel = QuestJournalPanel::new();
        assert!(!panel.open);
        assert_eq!(panel.tab, JournalTab::Active);
    }

    #[test]
    fn quest_journal_toggle() {
        let mut panel = QuestJournalPanel::new();
        assert!(!panel.open);

        panel.toggle();
        assert!(panel.open);

        panel.toggle();
        assert!(!panel.open);
    }

    #[test]
    fn objective_tracker_creation() {
        let tracker = ObjectiveTrackerWidget::new();
        assert_eq!(tracker.max_display, 5);
        assert!(tracker.show_progress_bars);
        assert!(!tracker.minimized);
    }

    #[test]
    fn dialogue_panel_creation() {
        let panel = DialoguePanel::new();
        assert!(!panel.visible);
        assert!(!panel.typewriter_active);
    }

    #[test]
    fn dialogue_set_dialogue() {
        let mut panel = DialoguePanel::new();
        panel.set_dialogue(
            "NPC",
            "Hello, adventurer!",
            vec![
                DialogueChoiceUI {
                    text: "Tell me more".to_string(),
                    index: 0,
                    is_available: true,
                },
            ],
        );

        assert_eq!(panel.speaker_name, "NPC");
        assert_eq!(panel.dialogue_text, "Hello, adventurer!");
        assert_eq!(panel.choices.len(), 1);
        assert!(panel.typewriter_active);
    }

    #[test]
    fn dialogue_typewriter_advance() {
        let mut panel = DialoguePanel::new();
        panel.set_dialogue("NPC", "Hello", vec![]);

        // Advance by 2 chars
        let still_active = panel.advance_typewriter(2);
        assert!(still_active);
        assert_eq!(panel.typewriter_index, 2);

        // Advance past end
        let still_active = panel.advance_typewriter(10);
        assert!(!still_active);
        assert_eq!(panel.typewriter_index, 5); // "Hello" is 5 chars
    }

    #[test]
    fn dialogue_skip_typewriter() {
        let mut panel = DialoguePanel::new();
        panel.set_dialogue("NPC", "Hello, world!", vec![]);

        panel.skip_typewriter();
        assert!(!panel.typewriter_active);
        assert_eq!(panel.typewriter_index, 13); // "Hello, world!" is 13 chars
    }

    #[test]
    fn reward_notification_creation() {
        let mut popup = RewardNotificationPopup::new();
        popup.add_notification("Gained 100 XP", RewardType::Experience, "⭐");

        assert_eq!(popup.notifications.len(), 1);
        assert_eq!(popup.notifications[0].message, "Gained 100 XP");
    }

    #[test]
    fn reward_notification_default_icons() {
        let mut popup = RewardNotificationPopup::new();
        popup.add_notification("Gold reward", RewardType::Gold, "");

        assert_eq!(popup.notifications[0].icon, "💰");
    }

    #[test]
    fn reward_notification_expiry() {
        let mut popup = RewardNotificationPopup::new();
        popup.add_notification("Test", RewardType::Experience, "⭐");

        // Update past lifetime
        popup.update(5.0);
        assert!(popup.notifications.is_empty());
    }

    #[test]
    fn reward_notification_max_visible() {
        let mut popup = RewardNotificationPopup::new();
        popup.max_visible = 3;

        for i in 0..5 {
            popup.add_notification(format!("Notification {}", i), RewardType::Gold, "💰");
        }

        assert_eq!(popup.notifications.len(), 3);
        // Oldest should be removed
        assert_eq!(popup.notifications[0].message, "Notification 2");
    }
}
