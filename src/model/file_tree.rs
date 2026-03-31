use std::collections::HashSet;

use super::File;
use super::CommitFile;

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

    // Collect (path_parts, file_index) and sort with directories before files
    let mut entries: Vec<(Vec<&str>, usize)> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let parts: Vec<&str> = f.name.split('/').collect();
            (parts, i)
        })
        .collect();
    entries.sort_by(|a, b| sort_dirs_first(&a.0, &b.0));

    // First pass: collect child file indices per directory path
    let mut dir_children: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (parts, file_idx) in &entries {
        for depth in 0..parts.len().saturating_sub(1) {
            let dir_path = parts[..=depth].join("/");
            dir_children.entry(dir_path).or_default().push(*file_idx);
        }
    }

    let mut nodes = Vec::new();

    // Root node — represents the entire tree
    let all_indices: Vec<usize> = (0..files.len()).collect();
    let root_collapsed = collapsed_dirs.contains(".");
    nodes.push(FileTreeNode {
        depth: 0,
        name: ".".to_string(),
        path: ".".to_string(),
        file_index: None,
        is_dir: true,
        child_file_indices: all_indices,
    });

    // If root is collapsed, return just the root node
    if root_collapsed {
        return nodes;
    }

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
                    depth: depth + 1, // +1 for root node
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
                depth: dir_parts.len() + 1, // +1 for root node
                name: file_name.to_string(),
                path: parts.join("/"),
                file_index: Some(*file_idx),
                is_dir: false,
                child_file_indices: Vec::new(),
            });
        }

        last_dirs = dir_parts.iter().map(|s| s.to_string()).collect();
    }

    compress_single_child_dirs(&mut nodes);
    nodes
}

/// Compress single-child directory chains into combined path nodes.
/// e.g., `apps` → `nextjs` → `src` becomes `apps/nextjs/src` as one node.
fn compress_single_child_dirs(nodes: &mut Vec<FileTreeNode>) {
    let mut i = 0;
    while i < nodes.len() {
        if !nodes[i].is_dir {
            i += 1;
            continue;
        }

        let d = nodes[i].depth;

        // Check if next node is a single dir child at depth d+1
        if i + 1 < nodes.len() && nodes[i + 1].is_dir && nodes[i + 1].depth == d + 1 {
            // Ensure no sibling at depth d+1 (only one direct child)
            let has_sibling = (i + 2..nodes.len())
                .take_while(|&j| nodes[j].depth > d)
                .any(|j| nodes[j].depth == d + 1);

            if !has_sibling {
                let child = nodes.remove(i + 1);
                if nodes[i].name == "." {
                    nodes[i].name = child.name;
                } else {
                    nodes[i].name = format!("{}/{}", nodes[i].name, child.name);
                }
                nodes[i].path = child.path;
                nodes[i].child_file_indices = child.child_file_indices;

                // Decrease depth of all descendants by 1
                let mut j = i + 1;
                while j < nodes.len() && nodes[j].depth > d {
                    nodes[j].depth -= 1;
                    j += 1;
                }
                continue; // re-check same node for further merges
            }
        }

        i += 1;
    }
}

/// Sort path parts so directories appear before files at each level,
/// then alphabetically within each group.
fn sort_dirs_first(a: &[&str], b: &[&str]) -> std::cmp::Ordering {
    for i in 0..a.len().min(b.len()) {
        if a[i] != b[i] {
            let a_is_dir = i < a.len() - 1;
            let b_is_dir = i < b.len() - 1;
            if a_is_dir != b_is_dir {
                return if a_is_dir {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            return a[i].cmp(b[i]);
        }
    }
    b.len().cmp(&a.len())
}

/// A node in the flattened commit file tree for display.
#[derive(Debug, Clone)]
pub struct CommitFileTreeNode {
    pub depth: usize,
    pub name: String,
    pub path: String,
    /// If this is a file node, the index into `Model.commit_files`.
    pub file_index: Option<usize>,
    pub is_dir: bool,
    /// For directory nodes: indices into `Model.commit_files` of all descendant files.
    pub child_file_indices: Vec<usize>,
}

/// Build a flat list of tree nodes from the commit file list.
pub fn build_commit_file_tree(
    files: &[CommitFile],
    collapsed_dirs: &HashSet<String>,
) -> Vec<CommitFileTreeNode> {
    if files.is_empty() {
        return Vec::new();
    }

    let mut entries: Vec<(Vec<&str>, usize)> = files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let parts: Vec<&str> = f.name.split('/').collect();
            (parts, i)
        })
        .collect();
    entries.sort_by(|a, b| sort_dirs_first(&a.0, &b.0));

    // First pass: collect child file indices per directory path
    let mut dir_children: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (parts, file_idx) in &entries {
        for depth in 0..parts.len().saturating_sub(1) {
            let dir_path = parts[..=depth].join("/");
            dir_children.entry(dir_path).or_default().push(*file_idx);
        }
    }

    let mut nodes = Vec::new();

    // Root node
    let all_indices: Vec<usize> = (0..files.len()).collect();
    let root_collapsed = collapsed_dirs.contains(".");
    nodes.push(CommitFileTreeNode {
        depth: 0,
        name: ".".to_string(),
        path: ".".to_string(),
        file_index: None,
        is_dir: true,
        child_file_indices: all_indices,
    });

    if root_collapsed {
        return nodes;
    }

    let mut last_dirs: Vec<String> = Vec::new();

    for (parts, file_idx) in &entries {
        let dir_parts = &parts[..parts.len() - 1];
        let file_name = parts[parts.len() - 1];

        let mut hidden = false;
        for depth in 0..dir_parts.len() {
            let ancestor_path = parts[..=depth].join("/");
            if collapsed_dirs.contains(&ancestor_path) {
                hidden = true;
                break;
            }
        }

        let common_prefix = last_dirs
            .iter()
            .zip(dir_parts.iter())
            .take_while(|(a, b)| a.as_str() == **b)
            .count();

        for (depth, dir) in dir_parts.iter().enumerate().skip(common_prefix) {
            let dir_path = parts[..=depth].join("/");
            let dir_hidden = (0..depth).any(|d| {
                let ancestor = parts[..=d].join("/");
                collapsed_dirs.contains(&ancestor)
            });

            if !dir_hidden {
                let children = dir_children.get(&dir_path).cloned().unwrap_or_default();
                nodes.push(CommitFileTreeNode {
                    depth: depth + 1, // +1 for root node
                    name: dir.to_string(),
                    path: dir_path.clone(),
                    file_index: None,
                    is_dir: true,
                    child_file_indices: children,
                });
            }

            if collapsed_dirs.contains(&dir_path) {
                break;
            }
        }

        if !hidden {
            nodes.push(CommitFileTreeNode {
                depth: dir_parts.len() + 1, // +1 for root node
                name: file_name.to_string(),
                path: parts.join("/"),
                file_index: Some(*file_idx),
                is_dir: false,
                child_file_indices: Vec::new(),
            });
        }

        last_dirs = dir_parts.iter().map(|s| s.to_string()).collect();
    }

    compress_single_child_commit_dirs(&mut nodes);
    nodes
}

/// Compress single-child directory chains for commit file trees.
fn compress_single_child_commit_dirs(nodes: &mut Vec<CommitFileTreeNode>) {
    let mut i = 0;
    while i < nodes.len() {
        if !nodes[i].is_dir {
            i += 1;
            continue;
        }

        let d = nodes[i].depth;

        if i + 1 < nodes.len() && nodes[i + 1].is_dir && nodes[i + 1].depth == d + 1 {
            let has_sibling = (i + 2..nodes.len())
                .take_while(|&j| nodes[j].depth > d)
                .any(|j| nodes[j].depth == d + 1);

            if !has_sibling {
                let child = nodes.remove(i + 1);
                if nodes[i].name == "." {
                    nodes[i].name = child.name;
                } else {
                    nodes[i].name = format!("{}/{}", nodes[i].name, child.name);
                }
                nodes[i].path = child.path;
                nodes[i].child_file_indices = child.child_file_indices;

                let mut j = i + 1;
                while j < nodes.len() && nodes[j].depth > d {
                    nodes[j].depth -= 1;
                    j += 1;
                }
                continue;
            }
        }

        i += 1;
    }
}
