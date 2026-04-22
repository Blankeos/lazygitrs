use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::config::Theme;
use crate::model::commit::{Commit, CommitStat, CommitStatus};

/// Render the read-only commit details panel into `rect`.  The panel is
/// deliberately non-focusable: it shows short-hash, author, email, date,
/// ref decorations, the full (wrapped) commit message, and a "N Changed Files
/// +A -B" summary when `stat` is available.
///
/// `full_message` is the unwrapped commit message (subject + body).  If only
/// the subject is known (e.g. when we haven't fetched the body yet), the
/// renderer falls back to `commit.name`.
pub fn render_commit_details(
    frame: &mut Frame,
    rect: Rect,
    commit: &Commit,
    stat: Option<&CommitStat>,
    full_message: Option<&str>,
    theme: &Theme,
    compact: bool,
    scroll: &mut u16,
) {
    let title_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "Commit Details",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ]);
    // Right-aligned hint showing the toggle key.  Placed on the top border.
    let hint_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("toggle ", Style::default().fg(theme.text_dimmed)),
        Span::styled(
            ".",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])
    .alignment(Alignment::Right);
    let block = Block::default()
        .title(title_line)
        .title(hint_line)
        .borders(Borders::ALL)
        .border_style(theme.inactive_border);

    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let message = full_message.unwrap_or(&commit.name);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(header_line(commit, theme));

    if !compact && !commit.author_email.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  ✉ ", Style::default().fg(theme.text_dimmed)),
            Span::styled(
                commit.author_email.clone(),
                Style::default().fg(theme.text),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("  # ", Style::default().fg(theme.text_dimmed)),
        Span::styled(
            commit.short_hash().to_string(),
            hash_style(commit, theme),
        ),
        Span::styled(
            format!(" {}", &commit.hash[commit.short_hash().len()..]),
            Style::default().fg(theme.text_dimmed),
        ),
    ]));

    if !compact && (!commit.refs.is_empty() || !commit.tags.is_empty()) {
        let mut spans = vec![Span::raw("  ")];
        for r in &commit.refs {
            let color = if r.starts_with("HEAD -> ") || r == "HEAD" {
                theme.ref_head
            } else if r.contains('/') {
                theme.ref_remote
            } else {
                theme.ref_local
            };
            spans.push(Span::styled(
                format!(" {} ", r),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
        }
        for t in &commit.tags {
            spans.push(Span::styled(
                format!(" {} ", t),
                Style::default().fg(theme.ref_tag).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }

    // Stat summary goes BEFORE the message so it's always visible without
    // scrolling past long commit bodies.  Only render when a meaningful stat
    // has been computed (files_changed > 0) — avoids showing "0 Changed Files"
    // while the background fetch is still running or on git errors.
    if let Some(s) = stat {
        if s.files_changed > 0 {
            lines.push(stat_line(s, theme));
        }
    }

    for segment in message.split('\n') {
        lines.push(Line::from(Span::styled(
            segment.to_string(),
            Style::default().fg(theme.text_strong),
        )));
    }

    // Estimate wrapped height so we can clamp the scroll offset (no scrolling
    // past the end of the content — browser-style).  `Paragraph::Wrap` wraps
    // at `inner.width`, so each logical line occupies
    // ceil(span_width / inner_width) visual rows (minimum 1).
    let iw = inner.width.max(1) as usize;
    let total_rows: usize = lines
        .iter()
        .map(|l| {
            let w: usize = l.spans.iter().map(|s| s.content.chars().count()).sum();
            if w == 0 { 1 } else { w.div_ceil(iw) }
        })
        .sum();
    let max_scroll = total_rows.saturating_sub(inner.height as usize) as u16;
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((*scroll, 0));
    frame.render_widget(para, inner);
}

fn hash_style(commit: &Commit, theme: &Theme) -> Style {
    match commit.status {
        CommitStatus::Unpushed => theme.commit_hash,
        CommitStatus::Pushed => Style::default().fg(theme.commit_hash_pushed),
        CommitStatus::Merged => Style::default().fg(theme.commit_hash_merged),
        _ => theme.commit_hash,
    }
}

fn header_line<'a>(commit: &'a Commit, theme: &Theme) -> Line<'a> {
    let initial = commit
        .author_name
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?');
    let avatar_color = avatar_color_for(&commit.author_email, theme);
    let date = format_date(commit.unix_timestamp);

    Line::from(vec![
        Span::styled(
            format!(" {} ", initial),
            Style::default()
                .fg(theme.text_strong)
                .bg(avatar_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            commit.author_name.clone(),
            Style::default()
                .fg(theme.text_strong)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(date, Style::default().fg(theme.text_dimmed)),
    ])
}

fn stat_line<'a>(stat: &CommitStat, theme: &Theme) -> Line<'a> {
    let files_label = if stat.files_changed == 1 {
        "Changed File"
    } else {
        "Changed Files"
    };
    Line::from(vec![
        Span::styled(
            format!("  {} {}  ", stat.files_changed, files_label),
            Style::default().fg(theme.text_dimmed),
        ),
        Span::styled(
            format!("+{}", stat.insertions),
            Style::default().fg(theme.change_added).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{}", stat.deletions),
            Style::default()
                .fg(theme.change_deleted)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn format_date(unix_ts: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    if unix_ts <= 0 {
        return String::new();
    }
    let dt = UNIX_EPOCH + Duration::from_secs(unix_ts as u64);
    // Format via chrono-free path: compute Y-m-d H:M locally-ish by converting
    // seconds since epoch.  We accept UTC display here to avoid pulling in a
    // tz library — matches the rest of this codebase's date treatment.
    let secs = dt
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    let (year, month, day, hour, minute) = civil_from_unix(secs as i64);
    let month_name = match month {
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
    format!("{} {}, {} {:02}:{:02}", month_name, day, year, hour, minute)
}

/// Very small civil-from-unix converter (UTC).  Matches Howard Hinnant's
/// algorithm.  Returns (year, month, day, hour, minute).
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

fn avatar_color_for(email: &str, theme: &Theme) -> ratatui::style::Color {
    // Pick a stable color from the graph palette based on a cheap hash of the
    // email so each author has their own recognisable block.
    let mut h: u32 = 2_166_136_261;
    for b in email.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16_777_619);
    }
    let palette = theme.graph_colors;
    palette[(h as usize) % palette.len()]
}

