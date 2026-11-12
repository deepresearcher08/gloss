//! Gloss facade: styles, widgets, signals and the application runtime.

pub use gloss_app::{App, HotReload, HotReloadError, KeyEvent, NamedKey, Vibrancy};
pub use gloss_core::*;
pub use gloss_widgets::{Document, Handlers, Node, Role};

/// Re-exported renderer types for advanced use.
pub mod render {
    pub use gloss_render::*;
}

/// Re-exported text engine for measuring text outside the tree.
pub mod text {
    pub use gloss_text::*;
}
