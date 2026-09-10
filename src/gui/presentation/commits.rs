use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
    full: bool,
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
        full,
        &cache.sub_commits,
    )
}

pub fn render_commit_list_window(
    model: &Model,
    theme: &Theme,
    cherry_picked: &[String],
    offset: usize,
    visible_height: usize,
    full: bool,
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
        full,
        &cache.commits,
    )
}

/// Row order matches lazygit: `hash date author graph refs/tags message`.
///
/// - `full` (maximised panel) shows the smart date + long author (17 wide),
///   otherwise the date is hidden and the author collapses to initials —
///   same compact/expanded responsiveness as lazygit.
/// - Fixed columns (hash/date/author) are padded to the visible window's max
///   width so rows align like lazygit's `RenderDisplayStrings`.
fn render_commits_window(
    commits: &[Commit],
    head_hash: &str,
    theme: &Theme,
    cherry_picked: &[String],
    offset: usize,
    visible_height: usize,
    full: bool,
    graph_layout: &GraphLayoutCache,
) -> Vec<ListItem<'static>> {
    let visible: Vec<(usize, &Commit)> = commits
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_height)
        .collect();
    if visible.is_empty() {
        return Vec::new();
    }

    // Precompute plain date/author strings for column alignment.
    let dates: Vec<String> = visible
        .iter()
        .map(|(_, c)| {
            if full {
                smart_date(c.unix_timestamp)
            } else {
                String::new()
            }
        })
        .collect();
    let authors: Vec<String> = visible
        .iter()
        .map(|(_, c)| {
            if full {
                long_author(&c.author_name, 17)
            } else {
                author_initials(&c.author_name)
            }
        })
        .collect();

    let date_w = dates.iter().map(|s| s.width()).max().unwrap_or(0);
    let author_w = authors.iter().map(|s| s.width()).max().unwrap_or(0);

    visible
        .iter()
        .enumerate()
        .map(|(vi, (i, commit))| {
            let graph_row = graph_layout.rows.get(*i);
            let is_head = commit.hash == *head_hash;
            let mut spans: Vec<Span<'static>> = Vec::new();

            // Hash (8, lazygit default) — color by push status.
            let is_cherry_picked = cherry_picked.iter().any(|h| *h == commit.hash);
            let hash_style = if is_cherry_picked {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                match commit.status {
                    CommitStatus::Unpushed => theme.commit_hash_unpushed,
                    CommitStatus::Pushed => theme.commit_hash,
                    CommitStatus::Merged => Style::default().fg(Color::Green),
                    CommitStatus::Rebasing
                    | CommitStatus::Selected
                    | CommitStatus::Conflicted
                    | CommitStatus::Reflog => Style::default().fg(Color::Blue),
                }
            };
            let hash = commit.short_hash().to_string();
            spans.push(Span::styled(pad_to(&hash, 8), hash_style));
            spans.push(Span::raw(" "));

            // Date (full mode only).
            if date_w > 0 {
                spans.push(Span::styled(pad_to(&dates[vi], date_w), theme.commit_date));
                spans.push(Span::raw(" "));
            }

            // Author — per-author color like lazygit's AuthorStyle.
            if author_w > 0 {
                let color = author_color(&commit.author_name);
                spans.push(Span::styled(
                    pad_to(&authors[vi], author_w),
                    Style::default().fg(color),
                ));
                spans.push(Span::raw(" "));
            }

            // Graph (after author, like lazygit).
            if let Some(row) = graph_row {
                spans.extend(graph::render_graph_spans(row, is_head, theme));
            } else {
                spans.push(Span::raw("  "));
            }

            // Tags/refs like lazygit (`commits.go:396`): compact shows bare
            // tags, maximised shows combined `(refs, tag: x)`. Color is
            // theme-based (`ref_tag`) rather than hardcoded magenta.
            let tag_style = Style::default()
                .fg(theme.ref_tag)
                .add_modifier(Modifier::BOLD);
            if full {
                let mut parts = commit.refs.clone();
                for tag in &commit.tags {
                    parts.push(format!("tag: {tag}"));
                }
                if !parts.is_empty() {
                    spans.push(Span::styled(format!("({}) ", parts.join(", ")), tag_style));
                }
            } else if !commit.tags.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", commit.tags.join(" ")),
                    tag_style,
                ));
            }

            // Message.
            spans.push(Span::styled(
                commit.name.clone(),
                Style::default().fg(theme.text_strong),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect()
}

fn pad_to(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

/// Smart date like lazygit: today shows time (`3:04PM`), else `02 Jan 06`.
fn smart_date(unix_ts: i64) -> String {
    if unix_ts <= 0 {
        return String::new();
    }
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(unix_ts);
    let (y1, m1, d1, _, _) = local_from_unix(unix_ts);
    let (y2, m2, d2, _, _) = local_from_unix(now_secs);
    if y1 == y2 && m1 == m2 && d1 == d2 {
        let (_, _, _, hour, minute) = local_from_unix(unix_ts);
        let (h12, ampm) = match hour {
            0 => (12, "AM"),
            1..=11 => (hour, "AM"),
            12 => (12, "PM"),
            _ => (hour - 12, "PM"),
        };
        format!("{h12}:{minute:02}{ampm}")
    } else {
        let month = match m1 {
            1 => "Jan",
            2 => "Feb",
            3 => "Mar",
            4 => "Apr",
            5 => "May",
            6 => "Jun",
            7 => "Jul",
            8 => "Aug",
            9 => "Sep",
            10 => "Oct",
            11 => "Nov",
            12 => "Dec",
            _ => "???",
        };
        format!("{:02} {} {:02}", d1, month, (y1 % 100 + 100) % 100)
    }
}

/// Local-time conversion via libc (matches lazygit's `In(now.Location())`).
/// Falls back to UTC (`civil_from_unix`) if `localtime_r` fails.
fn local_from_unix(secs: i64) -> (i64, u32, u32, u32, u32) {
    unsafe {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return civil_from_unix(secs);
        }
        (
            tm.tm_year as i64 + 1900,
            (tm.tm_mon + 1) as u32,
            tm.tm_mday as u32,
            tm.tm_hour as u32,
            tm.tm_min as u32,
        )
    }
}

fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, hour, minute)
}

/// Initials like lazygit: single word -> first 2 chars, two+ words -> first
/// char of first two words, wide first grapheme (emoji/CJK) -> first char.
fn author_initials(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let first = name.chars().next().unwrap_or_default();
    if first.width().unwrap_or(1) > 1 {
        return first.to_string();
    }
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() == 1 {
        parts[0].chars().take(2).collect()
    } else {
        parts
            .iter()
            .take(2)
            .filter_map(|p| p.chars().next())
            .collect()
    }
}

/// Padded/truncated author like lazygit's LongAuthor (left-aligned, `…`).
fn long_author(name: &str, length: usize) -> String {
    if name.width() <= length {
        return pad_to(name, length);
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in name.chars() {
        let cw = ch.width().unwrap_or(1);
        if w + cw > length - 1 {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    // Pad in case the ellipsis left us short (wide chars).
    pad_to(&out, length)
}

/// Stable per-author color like lazygit's AuthorStyle (HSL hashed -> RGB).
fn author_color(name: &str) -> Color {
    // FNV-1a for a stable hash without extra deps.
    let mut h: u32 = 2_166_136_261;
    for b in name.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    let hue = (h % 360) as f64;
    let sat = 0.6 + ((h >> 9) % 40) as f64 / 100.0;
    let light = 0.4 + ((h >> 15) % 20) as f64 / 100.0;
    let (r, g, b) = hsl_to_rgb(hue, sat, light);
    Color::Rgb(r, g, b)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
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
        let mut commits = vec![commit("bbbbbbbb", Some("aaaaaaaa"))];
        cache.update(&commits, 1);
        assert_eq!(cache.rows.len(), 1);

        commits.push(commit("aaaaaaaa", None));
        cache.update(&commits, 2);
        assert_eq!(cache.rows.len(), 2);
        assert_eq!(cache.revision, Some(2));
    }

    #[test]
    fn renders_only_requested_commit_window() {
        let mut model = Model::default();
        model.set_commits(vec![
            commit("cccccccc", Some("bbbbbbbb")),
            commit("bbbbbbbb", Some("aaaaaaaa")),
            commit("aaaaaaaa", None),
        ]);
        let mut cache = CommitListCache::default();

        let items =
            render_commit_list_window(&model, &Theme::default(), &[], 1, 1, false, &mut cache);

        assert_eq!(items.len(), 1);
    }

    #[test]
    fn author_initials_match_lazygit() {
        assert_eq!(author_initials(""), "");
        assert_eq!(author_initials("Jesse Duffield"), "JD");
        assert_eq!(author_initials("Jesse"), "Je");
        assert_eq!(author_initials("a"), "a");
    }

    #[test]
    fn long_author_pads_and_truncates() {
        assert_eq!(long_author("JD", 5), "JD   ");
        assert_eq!(long_author("Jesse Duffield Long Name", 8).width(), 8);
    }
}
