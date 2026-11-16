//! Read-only, keyboard-driven Linos Assets browser.
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub struct Browser {
    pub path: PathBuf,
    pub selected: usize,
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    bytes: Option<u64>,
}

impl Browser {
    pub fn new() -> Self {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut browser = Self {
            path: home,
            selected: 0,
            entries: Vec::new(),
        };
        let _ = browser.refresh();
        browser
    }

    pub fn open(&mut self, path: &Path) -> io::Result<()> {
        let canonical = path.canonicalize()?;
        if !canonical.is_dir() {
            return Err(io::Error::other("Not a directory"));
        }
        let entries = fs::read_dir(&canonical)?;
        let mut items = Vec::new();
        for entry in entries.flatten() {
            let meta = fs::symlink_metadata(entry.path()).ok();
            let name = entry
                .file_name()
                .to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .collect();
            let is_symlink = meta.as_ref().is_some_and(fs::Metadata::is_symlink);
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            let bytes = meta.as_ref().filter(|m| m.is_file()).map(fs::Metadata::len);
            items.push(Entry {
                name,
                is_dir,
                is_symlink,
                bytes,
            });
        }
        items.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.path = canonical;
        self.entries = items;
        self.selected = 0;
        Ok(())
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        let path = self.path.clone();
        self.open(&path)
    }

    pub fn parent(&mut self) -> io::Result<()> {
        if let Some(parent) = self.path.parent().map(Path::to_path_buf) {
            self.open(&parent)?;
        }
        Ok(())
    }

    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn activate(&mut self) -> io::Result<Option<PathBuf>> {
        let Some(item) = self.entries.get(self.selected) else {
            return Ok(None);
        };
        let path = self.path.join(&item.name);
        if path.is_dir() {
            self.open(&path)?;
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }

    pub fn display(&self) -> String {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut out = format!(
            "ASSETS  /  {}\n\nQUICK ACCESS    Home: {}    Root: /\n\n",
            self.path.display(),
            home.display()
        );
        out.push_str("  NAME                                       TYPE         SIZE\n");
        out.push_str("  ────────────────────────────────────────────────────────────\n");
        if self.entries.is_empty() {
            out.push_str("  This directory is empty.\n");
        }
        for (index, item) in self.entries.iter().enumerate().take(500) {
            let marker = if index == self.selected { "❯" } else { " " };
            let kind = if item.is_symlink {
                "Link"
            } else if item.is_dir {
                "Folder"
            } else {
                "File"
            };
            let size = item
                .bytes
                .map(|n| format!("{n} B"))
                .unwrap_or_else(|| " ".into());
            let name = item.name.chars().take(39).collect::<String>();
            out.push_str(&format!("{marker} {:<42} {:<12} {size}\n", name, kind));
        }
        if self.entries.len() > 500 {
            out.push_str("  More entries exist.\n");
        }
        out.push_str("\n↑↓ Select     Enter Open     Backspace Parent     /assets/path Jump\nRead-only browsing. No files are modified.");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_starts_in_home_and_can_navigate_parent() {
        let mut browser = Browser::new();
        assert!(browser.path.is_absolute());
        browser.open(Path::new("/")).unwrap();
        browser.parent().unwrap();
        assert_eq!(browser.path, Path::new("/"));
    }
}
