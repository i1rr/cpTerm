use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::util::strip_unc_prefix;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastSshConfig {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    pub path: String,
}

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub left_dir: PathBuf,
    pub right_dir: PathBuf,
    pub active_pane: PaneSide,
    #[serde(default = "default_theme_name")]
    pub theme_name: String,
    #[serde(default)]
    pub last_ssh: Option<LastSshConfig>,
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
            last_ssh: None,
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
        // confy 1.x uses "default-config" as the file name when None is passed, producing
        // "default-config.toml". We now use an explicit name ("cpt") so the file is "cpt.toml".
        // Migrate transparently: if "cpt.toml" doesn't exist but the legacy "default-config.toml"
        // does, load from the legacy file and save under the new name.
        let named_path = confy::get_configuration_file_path("cpt", Some("cpt")).ok();
        let legacy_path = confy::get_configuration_file_path("cpt", None).ok(); // default-config.toml

        let named_exists = named_path.as_ref().map_or(false, |p| p.exists());
        let legacy_exists = legacy_path.as_ref().map_or(false, |p| p.exists());

        let (mut config, needs_save): (Self, bool) = if !named_exists && legacy_exists {
            log::warn!("migrating session config from default-config.toml to cpt.toml");
            (confy::load("cpt", None).unwrap_or_default(), true)
        } else {
            (confy::load("cpt", Some("cpt")).unwrap_or_default(), false)
        };

        config.left_dir = strip_unc_prefix(config.left_dir);
        config.right_dir = strip_unc_prefix(config.right_dir);

        if needs_save {
            config.save();
        }

        config
    }

    pub fn save(&self) {
        if let Err(e) = confy::store("cpt", Some("cpt"), self) {
            log::warn!("failed to save session config: {}", e);
        }
    }
}
