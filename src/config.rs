use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::util::strip_unc_prefix;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub left_dir: PathBuf,
    pub right_dir: PathBuf,
    pub active_pane: PaneSide,
    #[serde(default = "default_theme_name")]
    pub theme_name: String,
}

fn default_theme_name() -> String {
    "default".to_string()
}

impl Default for SessionConfig {
    fn default() -> Self {
        let home = dirs_home();
        Self {
            left_dir: home.clone(),
            right_dir: home,
            active_pane: PaneSide::Left,
            theme_name: default_theme_name(),
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

impl SessionConfig {
    pub fn load() -> Self {
        let mut config: Self = confy::load("cpt", None).unwrap_or_default();
        config.left_dir = strip_unc_prefix(config.left_dir);
        config.right_dir = strip_unc_prefix(config.right_dir);
        config
    }

    pub fn save(&self) {
        if let Err(e) = confy::store("cpt", None, self) {
            log::warn!("failed to save session config: {}", e);
        }
    }
}
