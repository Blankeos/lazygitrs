use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

/// Adjust `scroll_offset` so that `selected` is visible within `visible_height`.
/// Only scrolls when the cursor would otherwise be outside the visible window.
pub fn ensure_visible(selected: usize, scroll_offset: &mut usize, visible_height: usize) {
    if visible_height == 0 {
        return;
    }
    if selected < *scroll_offset {
        *scroll_offset = selected;
    } else if selected >= *scroll_offset + visible_height {
        *scroll_offset = selected + 1 - visible_height;
    }
}

/// Scroll viewport by `delta` rows, clamping to valid range.
/// Does NOT change selection.
pub fn scroll_viewport(
    scroll_offset: &mut usize,
    delta: isize,
    list_len: usize,
    visible_height: usize,
) {
    if list_len <= visible_height {
        *scroll_offset = 0;
        return;
    }
    let max_offset = list_len.saturating_sub(visible_height);
    if delta < 0 {
        *scroll_offset = scroll_offset.saturating_sub(delta.unsigned_abs());
    } else {
        *scroll_offset = (*scroll_offset + delta as usize).min(max_offset);
    }
}

/// Calculate start offset and height of the scrollbar thumb within `scroll_area_size`.
/// Exact port of lazygit's `calcScrollbar` in `pkg/gocui/scrollbar.go`.
pub fn calc_scrollbar(
    list_size: usize,
    page_size: usize,
    position: usize,
    scroll_area_size: usize,
) -> (usize, usize) {
    let height = calc_scrollbar_height(list_size, page_size, scroll_area_size);
    let max_position = list_size.saturating_sub(page_size);
    if max_position == 0 {
        return (0, height);
    }
    if position >= max_position {
        return (scroll_area_size.saturating_sub(height), height);
    }
    // We only want to show the scrollbar at the top or bottom positions if we're at the end.
    // Hence the .ceil() and the -1 for pretending there's a smaller range than we actually have,
    // with the above condition ensuring we snap to the bottom once we're at the end of the list.
    let range = (scroll_area_size.saturating_sub(height).saturating_sub(1)) as f64;
    let start = (((position as f64) / (max_position as f64)) * range).ceil() as usize;
    (start, height)
}

/// Calculate the height of the scrollbar thumb.
/// Exact port of lazygit's `calcScrollbarHeight` in `pkg/gocui/scrollbar.go`.
pub fn calc_scrollbar_height(list_size: usize, page_size: usize, scroll_area_size: usize) -> usize {
    if page_size >= list_size || list_size == 0 {
        return scroll_area_size;
    }

    ((page_size as f64 / list_size as f64) * scroll_area_size as f64) as usize
}

/// Compute the start and end (inclusive) visual row indices on the border for the scrollbar.
/// Exact port of lazygit's `calcRealScrollbarStartEnd` in `pkg/gocui/gui.go`.
///
/// Returns `Some((real_start, real_end))` if the scrollbar should be displayed, or `None`.
pub fn calc_real_scrollbar_start_end(
    content_height: usize,
    visible_height: usize,
    scroll_offset: usize,
    can_scroll_past_bottom: bool,
    top_y: u16,
) -> Option<(u16, u16)> {
    let mut full_height = content_height;
    if can_scroll_past_bottom {
        full_height += visible_height;
    }

    if visible_height < 2 || (!can_scroll_past_bottom && visible_height >= full_height) {
        return None;
    }

    if content_height <= visible_height && scroll_offset == 0 {
        return None;
    }

    let scroll_area_size = visible_height.saturating_sub(1);
    let (scrollbar_start, scrollbar_height) =
        calc_scrollbar(full_height, visible_height, scroll_offset, scroll_area_size);

    let real_start = top_y + scrollbar_start as u16;
    let real_end = real_start + scrollbar_height as u16;

    Some((real_start, real_end))
}

/// Draw the scrollbar on the right border of `area` in the ratatui `Buffer`.
/// Uses the character '▐' (U+2590 Right Half Block) with the provided `border_style`, matching lazygit.
pub fn render_scrollbar(
    buf: &mut Buffer,
    area: Rect,
    content_height: usize,
    visible_height: usize,
    scroll_offset: usize,
    can_scroll_past_bottom: bool,
    border_style: Style,
) {
    if area.width == 0 || area.height < 2 || visible_height == 0 {
        return;
    }

    let right_x = area.x + area.width.saturating_sub(1);
    let top_y = area.y + 1;

    if let Some((start_y, end_y)) = calc_real_scrollbar_start_end(
        content_height,
        visible_height,
        scroll_offset,
        can_scroll_past_bottom,
        top_y,
    ) {
        let max_y = (top_y + visible_height as u16).min(area.bottom().saturating_sub(1));
        for y in top_y..max_y {
            if y >= start_y && y <= end_y {
                if let Some(cell) = buf.cell_mut((right_x, y)) {
                    cell.set_char('▐');
                    cell.set_style(border_style);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    #[test]
    fn test_calc_scrollbar_lazygit_compatibility() {
        // Test suite matches jesseduffield/lazygit/pkg/gocui/scrollbar_test.go verbatim
        let tests = vec![
            ("page size greater than list size", 5, 10, 0, 20, 0, 20),
            ("page size matches list size", 10, 10, 0, 20, 0, 20),
            ("page size half of list size", 10, 5, 0, 20, 0, 10),
            (
                "page size half of list size at scroll end",
                10,
                5,
                5,
                20,
                10,
                10,
            ),
            (
                "page size third of list size having scrolled half the way",
                15,
                5,
                5,
                21,
                7,
                7,
            ),
            (
                "page size third of list size having scrolled the full way",
                15,
                5,
                10,
                21,
                14,
                7,
            ),
            (
                "page size third of list size having scrolled by one",
                15,
                5,
                1,
                21,
                2,
                7,
            ),
            (
                "page size third of list size having scrolled up from the bottom by one",
                15,
                5,
                9,
                21,
                12,
                7,
            ),
        ];

        for (
            name,
            list_size,
            page_size,
            position,
            scroll_area_size,
            expected_start,
            expected_height,
        ) in tests
        {
            let (start, height) = calc_scrollbar(list_size, page_size, position, scroll_area_size);
            assert_eq!(
                start, expected_start,
                "Test '{}' failed: expected start {}, got {}",
                name, expected_start, start
            );
            assert_eq!(
                height, expected_height,
                "Test '{}' failed: expected height {}, got {}",
                name, expected_height, height
            );
        }
    }

    #[test]
    fn test_calc_real_scrollbar_start_end_short_content_no_scroll() {
        assert_eq!(
            calc_real_scrollbar_start_end(10, 20, 0, true, 1),
            None,
            "Short content with 0 scroll offset should not display scrollbar"
        );
        assert_eq!(
            calc_real_scrollbar_start_end(10, 20, 0, false, 1),
            None,
            "Short content without scroll past bottom should not display scrollbar"
        );
    }

    #[test]
    fn test_calc_real_scrollbar_start_end_scroll_past_bottom() {
        let res = calc_real_scrollbar_start_end(100, 20, 0, true, 1);
        assert!(res.is_some());
        let (start, end) = res.unwrap();
        assert_eq!(start, 1);
        assert_eq!(end, 4);

        // At end of scrollable content
        let res_end = calc_real_scrollbar_start_end(100, 20, 100, true, 1);
        assert!(res_end.is_some());
        let (start_end, end_end) = res_end.unwrap();
        assert_eq!(start_end, 17);
        assert_eq!(end_end, 20);
    }

    #[test]
    fn test_render_scrollbar_writes_right_half_blocks() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));
        let area = Rect::new(0, 0, 30, 10);
        let border_style = Style::default().fg(Color::Cyan);

        render_scrollbar(&mut buf, area, 100, 8, 0, true, border_style);

        // Check the right border column at x = 29
        let right_x = 29;
        assert_eq!(buf[(right_x, 1)].symbol(), "▐");
        assert_eq!(buf[(right_x, 1)].fg, Color::Cyan);
    }
}
