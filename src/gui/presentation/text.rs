use std::borrow::Cow;

/// Repository text is data, not terminal instructions. Strip escapes before
/// measuring/clipping it: clipping can otherwise remove an SGR reset while
/// leaving the opening escape to affect the rest of the terminal frame.
/// Keep newlines and expand tabs for multiline messages, but no other controls.
pub(crate) fn plain_text(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| c.is_control() && c != '\n') {
        return Cow::Borrowed(text);
    }
    let input = if text.contains('\t') {
        Cow::Owned(text.replace('\t', "    "))
    } else {
        Cow::Borrowed(text)
    };
    let mut clean = strip_ansi_escapes::strip_str(input);
    clean.retain(|c| !c.is_control() || c == '\n');
    Cow::Owned(clean)
}

#[cfg(test)]
mod tests {
    use super::plain_text;

    #[test]
    fn clean_text_is_borrowed_including_multiline_unicode() {
        for text in ["", "ordinary commit", "héllo 世界\n\nmessage body"] {
            assert!(
                matches!(plain_text(text), std::borrow::Cow::Borrowed(s) if std::ptr::eq(s, text))
            );
        }
    }

    #[test]
    fn removes_terminal_instructions_but_preserves_message_text() {
        assert_eq!(
            plain_text("Add Vike skill — \x1b[4mhttps://vike.dev/ai#skill\x1b[24m"),
            "Add Vike skill — https://vike.dev/ai#skill"
        );
        assert_eq!(
            plain_text("\x1b[4munterminated style"),
            "unterminated style"
        );
        assert_eq!(plain_text("text\x1b[4"), "text");
        assert_eq!(plain_text("a\x1b[2Jb\x1b[Hc"), "abc");
        assert_eq!(
            plain_text("\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\"),
            "link"
        );
        assert_eq!(plain_text("\x1b]0;window title\x07text"), "text");
        assert_eq!(
            plain_text("héllo 世界\n\tmessage\x07\r"),
            "héllo 世界\n    message"
        );
    }
}
