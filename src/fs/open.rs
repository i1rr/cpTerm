use std::path::Path;

pub fn open_file(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))
}
