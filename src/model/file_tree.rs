use std::collections::HashSet;

use super::File;

/// A node in the flattened file tree for display.
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    /// Indentation depth (0 = root level).
    pub depth: usize,
    /// Display name (just the directory or file name, not the full path).
    pub name: String,
    /// Full directory path (e.g. "src/gui") for directories, used for collapse tracking.
    pub path: String,
    /// If this is a file node, the index into `Model.files`.
    pub file_index: Option<usize>,
    pub is_dir: bool,
    /// For directory nodes: indices into `Model.files` of all descendant files.
    pub child_file_indices: Vec<usize>,
}

/// Build a flat list of tree nodes from the file list.
/// `collapsed_dirs` controls which directories are collapsed (hidden children).
pub fn build_file_tree(files: &[File], collapsed_dirs: &HashSet<String>) -> Vec<FileTreeNode> {
    if files.is_empty() {
        return Vec::new();
    }

    // Collect (path_parts, file_index) and sort by path
    let mut entries: Vec<(Vec<&str>, usize)> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let parts: Vec<&str> = f.name.split('/').collect();
            (parts, i)
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // First pass: collect child file indices per directory path
    let mut dir_children: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (parts, file_idx) in &entries {
        for depth in 0..parts.len().saturating_sub(1) {
            let dir_path = parts[..=depth].join("/");
            dir_children.entry(dir_path).or_default().push(*file_idx);
        }
    }

    let mut nodes = Vec::new();
    let mut last_dirs: Vec<String> = Vec::new();

    for (parts, file_idx) in &entries {
        let dir_parts = &parts[..parts.len() - 1];
        let file_name = parts[parts.len() - 1];

        // Check if any ancestor directory is collapsed — if so, skip this file
        let mut hidden = false;
        for depth in 0..dir_parts.len() {
            let ancestor_path = parts[..=depth].join("/");
            if collapsed_dirs.contains(&ancestor_path) {
                // Only hide if this file is deeper than the collapsed dir itself
                // (the collapsed dir node is still shown)
                hidden = true;
                break;
            }
        }

        // Emit directory nodes for any new directories
        let common_prefix = last_dirs
            .iter()
            .zip(dir_parts.iter())
            .take_while(|(a, b)| a.as_str() == **b)
            .count();

        // Add new directory levels (but only if not hidden by a collapsed ancestor)
        for (depth, dir) in dir_parts.iter().enumerate().skip(common_prefix) {
            let dir_path = parts[..=depth].join("/");

            // Check if THIS directory is hidden by a collapsed ancestor above it
            let dir_hidden = (0..depth).any(|d| {
                let ancestor = parts[..=d].join("/");
                collapsed_dirs.contains(&ancestor)
            });

            if !dir_hidden {
                let children = dir_children.get(&dir_path).cloned().unwrap_or_default();
                nodes.push(FileTreeNode {
                    depth,
                    name: dir.to_string(),
                    path: dir_path.clone(),
                    file_index: None,
                    is_dir: true,
                    child_file_indices: children,
                });
            }

            // If this directory is collapsed, don't process deeper dirs
            if collapsed_dirs.contains(&dir_path) {
                break;
            }
        }

        if !hidden {
            nodes.push(FileTreeNode {
                depth: dir_parts.len(),
                name: file_name.to_string(),
                path: parts.join("/"),
                file_index: Some(*file_idx),
                is_dir: false,
                child_file_indices: Vec::new(),
            });
        }

        last_dirs = dir_parts.iter().map(|s| s.to_string()).collect();
    }

    nodes
}
