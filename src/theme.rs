use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// All configurable UI colors, organized by component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    // Explorer pane
    pub border_focused: Color,
    pub border_unfocused: Color,
    pub dir_fg: Color,
    pub selected_fg: Color,
    pub selected_bg: Color,

    // Status bar
    pub status_bg: Color,
    pub status_fg: Color,
    pub path_fg: Color,
    pub selection_fg: Color,
    pub filter_fg: Color,

    // Command bar
    pub cmdbar_bg: Color,
    pub cmdbar_fg: Color,
    pub fkey_fg: Color,
    pub fkey_bg: Color,
    pub hint_fg: Color,
    pub input_fg: Color,
    pub help_fg: Color,

    // Dialogs
    pub confirm_border: Color,
    pub confirm_fg: Color,
    pub conflict_border: Color,
    pub conflict_fg: Color,
    pub error_border: Color,
    pub error_fg: Color,
    pub info_border: Color,
    pub info_fg: Color,
}

pub const BUILTIN_THEMES: &[&str] = &["default", "muted"];
pub const FIELD_COUNT: usize = 25;

/// (section_name, field_label) for each of the 25 theme fields, in order.
pub const FIELD_INFO: [(&str, &str); FIELD_COUNT] = [
    ("Explorer", "border focused"),
    ("Explorer", "border unfocused"),
    ("Explorer", "directory"),
    ("Explorer", "selected fg"),
    ("Explorer", "selected bg"),
    ("Status Bar", "background"),
    ("Status Bar", "foreground"),
    ("Status Bar", "path"),
    ("Status Bar", "selection"),
    ("Status Bar", "filter"),
    ("Command Bar", "background"),
    ("Command Bar", "foreground"),
    ("Command Bar", "F-key fg"),
    ("Command Bar", "F-key bg"),
    ("Command Bar", "hint"),
    ("Command Bar", "input"),
    ("Command Bar", "help text"),
    ("Dialogs", "confirm border"),
    ("Dialogs", "confirm text"),
    ("Dialogs", "conflict border"),
    ("Dialogs", "conflict text"),
    ("Dialogs", "error border"),
    ("Dialogs", "error text"),
    ("Dialogs", "info border"),
    ("Dialogs", "info text"),
];

// ── Color helpers ────────────────────────────────────────

/// Convert an ANSI 256 index to the best ratatui Color variant.
/// Uses named variants for 0-15 so TOML serialization is readable.
pub fn indexed_to_color(idx: u8) -> Color {
    match idx {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        15 => Color::White,
        n => Color::Indexed(n),
    }
}

/// Convert a ratatui Color to a 256-palette index.
pub fn color_to_index(c: Color) -> u8 {
    match c {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        Color::Indexed(n) => n,
        _ => 0,
    }
}

/// Pick a contrasting foreground (black or white) for legibility on a given indexed bg.
pub fn contrast_fg(idx: u8) -> Color {
    match idx {
        0..=6 | 8 => Color::White,
        7 | 9..=15 => Color::Black,
        16..=231 => {
            let v = idx - 16;
            let r = (v / 36) as u16;
            let g = ((v % 36) / 6) as u16;
            let b = (v % 6) as u16;
            if r * 2 + g * 3 + b > 8 {
                Color::Black
            } else {
                Color::White
            }
        }
        232..=243 => Color::White,
        244..=255 => Color::Black,
    }
}

/// Human-readable display name for a color.
pub fn color_display_name(c: Color) -> String {
    match c {
        Color::Black => "Black".into(),
        Color::Red => "Red".into(),
        Color::Green => "Green".into(),
        Color::Yellow => "Yellow".into(),
        Color::Blue => "Blue".into(),
        Color::Magenta => "Magenta".into(),
        Color::Cyan => "Cyan".into(),
        Color::Gray => "Gray".into(),
        Color::DarkGray => "DarkGray".into(),
        Color::LightRed => "LightRed".into(),
        Color::LightGreen => "LightGreen".into(),
        Color::LightYellow => "LightYellow".into(),
        Color::LightBlue => "LightBlue".into(),
        Color::LightMagenta => "LightMagenta".into(),
        Color::LightCyan => "LightCyan".into(),
        Color::White => "White".into(),
        Color::Indexed(n) if n >= 16 && n < 232 => {
            let v = n - 16;
            let r = v / 36;
            let g = (v % 36) / 6;
            let b = v % 6;
            format!("{} ({}:{}:{})", n, r, g, b)
        }
        Color::Indexed(n) if n >= 232 => format!("{} (gray {})", n, n - 232),
        Color::Indexed(n) => format!("{}", n),
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "?".into(),
    }
}

// ── Theme impls ──────────────────────────────────────────

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_focused: Color::Cyan,
            border_unfocused: Color::DarkGray,
            dir_fg: Color::Blue,
            selected_fg: Color::Black,
            selected_bg: Color::Yellow,
            status_bg: Color::DarkGray,
            status_fg: Color::White,
            path_fg: Color::Cyan,
            selection_fg: Color::Yellow,
            filter_fg: Color::Green,
            cmdbar_bg: Color::Black,
            cmdbar_fg: Color::White,
            fkey_fg: Color::Black,
            fkey_bg: Color::Cyan,
            hint_fg: Color::Cyan,
            input_fg: Color::Yellow,
            help_fg: Color::DarkGray,
            confirm_border: Color::Yellow,
            confirm_fg: Color::White,
            conflict_border: Color::LightRed,
            conflict_fg: Color::LightRed,
            error_border: Color::Red,
            error_fg: Color::Red,
            info_border: Color::Green,
            info_fg: Color::Green,
        }
    }
}

impl Theme {
    pub fn muted() -> Self {
        Self {
            border_focused: Color::White,
            border_unfocused: Color::DarkGray,
            dir_fg: Color::Cyan,
            selected_fg: Color::White,
            selected_bg: Color::DarkGray,
            status_bg: Color::DarkGray,
            status_fg: Color::White,
            path_fg: Color::White,
            selection_fg: Color::LightCyan,
            filter_fg: Color::LightGreen,
            cmdbar_bg: Color::Black,
            cmdbar_fg: Color::White,
            fkey_fg: Color::Black,
            fkey_bg: Color::White,
            hint_fg: Color::White,
            input_fg: Color::LightCyan,
            help_fg: Color::DarkGray,
            confirm_border: Color::White,
            confirm_fg: Color::White,
            conflict_border: Color::Yellow,
            conflict_fg: Color::Yellow,
            error_border: Color::LightRed,
            error_fg: Color::LightRed,
            info_border: Color::LightGreen,
            info_fg: Color::LightGreen,
        }
    }

    /// Load a theme by name: tries custom file first, then built-in.
    pub fn by_name(name: &str) -> Self {
        if let Some(theme) = Self::load_from_file(name) {
            return theme;
        }
        match name {
            "muted" => Self::muted(),
            _ => Self::default(),
        }
    }

    pub fn is_builtin(name: &str) -> bool {
        BUILTIN_THEMES.contains(&name)
    }

    /// All available theme names: built-in first, then custom.
    pub fn all_theme_names() -> Vec<String> {
        let mut names: Vec<String> = BUILTIN_THEMES.iter().map(|s| s.to_string()).collect();
        for custom in Self::list_custom() {
            if !names.contains(&custom) {
                names.push(custom);
            }
        }
        names
    }

    /// Next theme name in the cycle.
    pub fn next_theme_name(current: &str) -> String {
        let names = Self::all_theme_names();
        if names.is_empty() {
            return "default".to_string();
        }
        let idx = names.iter().position(|n| n == current).unwrap_or(0);
        names[(idx + 1) % names.len()].clone()
    }

    // ── Field access by index ────────────────────────────

    pub fn get_field(&self, index: usize) -> Color {
        match index {
            0 => self.border_focused,
            1 => self.border_unfocused,
            2 => self.dir_fg,
            3 => self.selected_fg,
            4 => self.selected_bg,
            5 => self.status_bg,
            6 => self.status_fg,
            7 => self.path_fg,
            8 => self.selection_fg,
            9 => self.filter_fg,
            10 => self.cmdbar_bg,
            11 => self.cmdbar_fg,
            12 => self.fkey_fg,
            13 => self.fkey_bg,
            14 => self.hint_fg,
            15 => self.input_fg,
            16 => self.help_fg,
            17 => self.confirm_border,
            18 => self.confirm_fg,
            19 => self.conflict_border,
            20 => self.conflict_fg,
            21 => self.error_border,
            22 => self.error_fg,
            23 => self.info_border,
            24 => self.info_fg,
            _ => Color::Reset,
        }
    }

    pub fn set_field(&mut self, index: usize, color: Color) {
        match index {
            0 => self.border_focused = color,
            1 => self.border_unfocused = color,
            2 => self.dir_fg = color,
            3 => self.selected_fg = color,
            4 => self.selected_bg = color,
            5 => self.status_bg = color,
            6 => self.status_fg = color,
            7 => self.path_fg = color,
            8 => self.selection_fg = color,
            9 => self.filter_fg = color,
            10 => self.cmdbar_bg = color,
            11 => self.cmdbar_fg = color,
            12 => self.fkey_fg = color,
            13 => self.fkey_bg = color,
            14 => self.hint_fg = color,
            15 => self.input_fg = color,
            16 => self.help_fg = color,
            17 => self.confirm_border = color,
            18 => self.confirm_fg = color,
            19 => self.conflict_border = color,
            20 => self.conflict_fg = color,
            21 => self.error_border = color,
            22 => self.error_fg = color,
            23 => self.info_border = color,
            24 => self.info_fg = color,
            _ => {}
        }
    }

    // ── File I/O ─────────────────────────────────────────

    pub fn themes_dir() -> Option<PathBuf> {
        let config_path = confy::get_configuration_file_path("cpt", None).ok()?;
        Some(config_path.parent()?.join("themes"))
    }

    fn list_custom() -> Vec<String> {
        let Some(dir) = Self::themes_dir() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml")
                && let Some(stem) = path.file_stem()
            {
                names.push(stem.to_string_lossy().to_string());
            }
        }
        names.sort();
        names
    }

    fn load_from_file(name: &str) -> Option<Self> {
        let dir = Self::themes_dir()?;
        let path = dir.join(format!("{}.toml", name));
        let content = std::fs::read_to_string(&path).ok()?;
        match toml::from_str(&content) {
            Ok(theme) => Some(theme),
            Err(e) => {
                log::warn!("failed to parse theme '{}': {}", name, e);
                None
            }
        }
    }

    pub fn save_to_file(&self, name: &str) -> Result<PathBuf, String> {
        let dir =
            Self::themes_dir().ok_or_else(|| "could not determine themes directory".to_string())?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create themes directory: {}", e))?;
        let path = dir.join(format!("{}.toml", name));
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize theme: {}", e))?;
        std::fs::write(&path, content).map_err(|e| format!("failed to write theme file: {}", e))?;
        Ok(path)
    }

    pub fn delete_file(name: &str) -> Result<(), String> {
        let dir =
            Self::themes_dir().ok_or_else(|| "could not determine themes directory".to_string())?;
        let path = dir.join(format!("{}.toml", name));
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("failed to delete theme file: {}", e))?;
        }
        Ok(())
    }
}
