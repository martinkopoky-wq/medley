//! Dockable panels. Each module exposes `pub fn ui(ui: &mut egui::Ui, state: &mut AppState)`.

pub mod accounts;
pub mod ai_studio;
pub mod appearance;
pub mod collection;
pub mod library;
pub mod now_playing;
pub mod queue;
pub mod search;
pub mod settings;
pub mod tracks;

use crate::app::AppState;
use eframe::egui::{self, Id, Ui, WidgetText};
use egui_dock::TabViewer;
use medley_core::layout::PanelKind;

pub struct Viewer<'a> {
    pub state: &'a mut AppState,
}

impl TabViewer for Viewer<'_> {
    type Tab = PanelKind;

    fn id(&mut self, tab: &mut PanelKind) -> Id {
        Id::new(("medley.panel", tab.slug()))
    }

    fn title(&mut self, tab: &mut PanelKind) -> WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut PanelKind) {
        let state = &mut *self.state;
        ui.push_id(tab.slug(), |ui| match tab {
            PanelKind::Library => library::ui(ui, state),
            PanelKind::Search => search::ui(ui, state),
            PanelKind::Collection => collection::ui(ui, state),
            PanelKind::NowPlaying => now_playing::ui(ui, state),
            PanelKind::Queue => queue::ui(ui, state),
            PanelKind::AiStudio => ai_studio::ui(ui, state),
            PanelKind::Accounts => accounts::ui(ui, state),
            PanelKind::Appearance => appearance::ui(ui, state),
            PanelKind::Settings => settings::ui(ui, state),
        });
    }

    /// Browse is the main content area and always stays.
    fn is_closeable(&self, tab: &PanelKind) -> bool {
        *tab != PanelKind::Collection
    }

    fn allowed_in_windows(&self, _tab: &mut PanelKind) -> bool {
        false
    }

    /// Panels manage their own scrolling (virtualized lists, fixed headers).
    fn scroll_bars(&self, _tab: &PanelKind) -> [bool; 2] {
        [false, false]
    }
}

/// Standard vertical scroll area for panel content that isn't a long list.
pub fn scroll<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, add).inner
}
