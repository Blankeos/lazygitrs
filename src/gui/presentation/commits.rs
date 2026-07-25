use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;
use crate::model::commit::{Commit, CommitStatus};

use super::graph;

#[derive(Default)]
pub struct CommitListCache {
    commits: GraphLayoutCache,
    sub_commits: GraphLayoutCache,
}

#[derive(Default)]
struct GraphLayoutCache {
    revision: Option<u64>,
    commit_count: usize,
    rows: Vec<graph::GraphRow>,
}

impl GraphLayoutCache {
    fn update(&mut self, commits: &[Commit], revision: u64) {
        if self.revision == Some(revision) && self.commit_count == commits.len() {
            return;
        }

        let graph_input: Vec<(String, Vec<String>)> = commits
            .iter()
            .map(|commit| (commit.hash.clone(), commit.parents.clone()))
            .collect();
        self.rows = graph::compute_graph(&graph_input);
        self.revision = Some(revision);
        self.commit_count = commits.len();
    }
}

pub fn render_sub_commit_list_window(
    model: &Model,
    theme: &Theme,
    offset: usize,
    visible_height: usize,
    cache: &mut CommitListCache,
) -> Vec<ListItem<'static>> {
    cache
        .sub_commits
        .update(&model.sub_commits, model.sub_commits_revision);
    render_commits_window(
        &model.sub_commits,
        &model.head_hash,
        theme,
        &[],
        offset,
        visible_height,
        &cache.sub_commits,
    )
}

pub fn render_commit_list_window(
    model: &Model,
    theme: &Theme,
    cherry_picked: &[String],
    offset: usize,
    visible_height: usize,
    cache: &mut CommitListCache,
) -> Vec<ListItem<'static>> {
    cache.commits.update(&model.commits, model.commits_revision);
    render_commits_window(
        &model.commits,
        &model.head_hash,
        theme,
        cherry_picked,
        offset,
        visible_height,
        &cache.commits,
    )
}

fn render_commits_window(
    commits: &[Commit],
    head_hash: &str,
    theme: &Theme,
    cherry_picked: &[String],
    offset: usize,
    visible_height: usize,
    graph_layout: &GraphLayoutCache,
) -> Vec<ListItem<'static>> {
    commits
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .map(|(i, commit)| {
            let graph_row = graph_layout.rows.get(i);
            let is_head = commit.hash == *head_hash;

            // Start with graph spans (per-row width; no global pad).
            let mut spans: Vec<Span<'static>> = if let Some(row) = graph_row {
                graph::render_graph_spans(row, is_head, theme)
            } else {
                vec![Span::raw(" ")]
            };

            // Hash — color by push status, overridden to cyan+bold if cherry-picked
            let is_cherry_picked = cherry_picked.iter().any(|h| *h == commit.hash);
            let hash_style = if is_cherry_picked {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                match commit.status {
                    CommitStatus::Unpushed => theme.commit_hash,
                    CommitStatus::Pushed => Style::default().fg(theme.commit_hash_pushed),
                    CommitStatus::Merged => Style::default().fg(theme.commit_hash_merged),
                    _ => theme.commit_hash,
                }
            };
            spans.push(Span::styled(
                format!("{} ", commit.short_hash()),
                hash_style,
            ));

            // Ref decorations (HEAD -> main, origin/main, etc.)
            for r in &commit.refs {
                let (label, color) = if r.starts_with("HEAD -> ") {
                    (r.clone(), theme.ref_head)
                } else if r == "HEAD" {
                    (r.clone(), theme.ref_head)
                } else if r.contains('/') {
                    // Remote ref like origin/main
                    (r.clone(), theme.ref_remote)
                } else {
                    // Local branch
                    (r.clone(), theme.ref_local)
                };
                spans.push(Span::styled(
                    format!("({}) ", label),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ));
            }

            // Tags (before message so they're visible in compact views)
            for tag in &commit.tags {
                spans.push(Span::styled(
                    format!("[{}] ", tag),
                    Style::default()
                        .fg(theme.ref_tag)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            // Commit message
            spans.push(Span::styled(
                commit.name.clone(),
                Style::default().fg(theme.text_strong),
            ));

            // Author (compact)
            spans.push(Span::styled(
                format!(" {}", commit.author_name),
                theme.commit_author,
            ));

            ListItem::new(Line::from(spans))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::commit::{CommitStatus, Divergence};

    fn commit(hash: &str, parent: Option<&str>) -> Commit {
        Commit {
            hash: hash.to_string(),
            name: hash.to_string(),
            status: CommitStatus::Pushed,
            action: String::new(),
            tags: Vec::new(),
            refs: Vec::new(),
            extra_info: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            unix_timestamp: 0,
            parents: parent.into_iter().map(str::to_string).collect(),
            divergence: Divergence::None,
        }
    }

    #[test]
    fn graph_layout_cache_rebuilds_when_revision_changes() {
        let mut cache = GraphLayoutCache::default();
        let mut commits = vec![commit("bbbbbbb", Some("aaaaaaa"))];
        cache.update(&commits, 1);
        assert_eq!(cache.rows.len(), 1);

        commits.push(commit("aaaaaaa", None));
        cache.update(&commits, 2);
        assert_eq!(cache.rows.len(), 2);
        assert_eq!(cache.revision, Some(2));
    }

    #[test]
    fn renders_only_requested_commit_window() {
        let mut model = Model::default();
        model.set_commits(vec![
            commit("ccccccc", Some("bbbbbbb")),
            commit("bbbbbbb", Some("aaaaaaa")),
            commit("aaaaaaa", None),
        ]);
        let mut cache = CommitListCache::default();

        let items = render_commit_list_window(&model, &Theme::default(), &[], 1, 1, &mut cache);

        assert_eq!(items.len(), 1);
    }
}
