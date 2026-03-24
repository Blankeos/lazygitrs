use std::collections::HashSet;

/// Tracks state for patch building mode.
/// Users can select individual hunks from commits to build a custom patch.
#[derive(Debug, Default)]
pub struct PatchBuildingState {
    /// Whether patch building mode is active.
    pub active: bool,
    /// The commit hash being patched from.
    pub source_commit: String,
    /// Files included in the patch (by file path).
    pub selected_files: HashSet<String>,
    /// Hunk indices selected per file: (file_path, hunk_indices).
    pub selected_hunks: Vec<(String, HashSet<usize>)>,
}

impl PatchBuildingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter(&mut self, commit_hash: String) {
        self.active = true;
        self.source_commit = commit_hash;
        self.selected_files.clear();
        self.selected_hunks.clear();
    }

    pub fn exit(&mut self) {
        self.active = false;
        self.source_commit.clear();
        self.selected_files.clear();
        self.selected_hunks.clear();
    }

    pub fn toggle_file(&mut self, file_path: &str) {
        if self.selected_files.contains(file_path) {
            self.selected_files.remove(file_path);
        } else {
            self.selected_files.insert(file_path.to_string());
        }
    }

    pub fn has_selections(&self) -> bool {
        !self.selected_files.is_empty()
    }
}
