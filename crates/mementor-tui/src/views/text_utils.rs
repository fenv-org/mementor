use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Truncate a string to fit within `max_width` terminal cells.
///
/// If the string's display width exceeds `max_width`, it is truncated at a
/// character boundary and `"..."` (3 cells) is appended. The total display
/// width of the returned string (including the ellipsis) will not exceed
/// `max_width`.
///
/// When `max_width < 4`, the ellipsis is omitted and the string is simply
/// truncated to fit.
pub fn truncate(s: &str, max_width: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_width {
        return s.to_owned();
    }

    let ellipsis = "...";
    let ellipsis_width = 3;

    if max_width < ellipsis_width + 1 {
        // Not enough room for even one char + ellipsis; just hard-truncate.
        return truncate_to_width(s, max_width);
    }

    let budget = max_width - ellipsis_width;
    let mut result = truncate_to_width(s, budget);
    result.push_str(ellipsis);
    result
}

/// Truncate a string to fit within exactly `max_width` terminal cells,
/// cutting at a character boundary. No ellipsis is added.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > max_width {
            break;
        }
        result.push(ch);
        used += ch_width;
    }
    result
}

/// Wrap a string into lines that each fit within `width` terminal cells.
///
/// Splits on character boundaries, respecting display width. Does not break
/// within a multi-width character — if a wide character would exceed the line
/// width, it starts on the next line.
pub fn wrap_str(s: &str, width: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    let mut result = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width && !line.is_empty() {
            result.push(std::mem::take(&mut line));
            used = 0;
        }
        line.push(ch);
        used += ch_width;
    }
    if !line.is_empty() {
        result.push(line);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    // -----------------------------------------------------------------------
    // Helper: assert the display width of a string.
    // -----------------------------------------------------------------------
    fn display_width(s: &str) -> usize {
        UnicodeWidthStr::width(s)
    }

    // =======================================================================
    // truncate() tests
    // =======================================================================

    #[test]
    fn truncate_ascii_no_truncation() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_ascii_with_truncation() {
        let result = truncate("hello world", 8);
        // budget = 8 - 3 = 5 chars, then "..."
        assert_eq!(result, "hello...");
        assert!(display_width(&result) <= 8);
    }

    #[test]
    fn truncate_cjk_no_truncation() {
        // Each CJK char is 2 cells wide. "안녕" = 4 cells.
        let s = "안녕";
        assert_eq!(display_width(s), 4);
        assert_eq!(truncate(s, 4), "안녕");
        assert_eq!(truncate(s, 10), "안녕");
    }

    #[test]
    fn truncate_cjk_with_truncation() {
        // "안녕하세요" = 10 cells. Truncate to 8: budget = 5, fits 2 CJK chars (4 cells).
        let s = "안녕하세요";
        assert_eq!(display_width(s), 10);
        let result = truncate(s, 8);
        assert_eq!(result, "안녕...");
        assert!(display_width(&result) <= 8);
    }

    #[test]
    fn truncate_cjk_boundary_cannot_fit_wide_char() {
        // "漢字テスト" = 10 cells. Truncate to 7: budget = 4, fits 2 CJK chars (4 cells).
        let s = "漢字テスト";
        assert_eq!(display_width(s), 10);
        let result = truncate(s, 7);
        assert_eq!(result, "漢字...");
        assert_eq!(display_width(&result), 7);
    }

    #[test]
    fn truncate_mixed_ascii_cjk() {
        // "hello世界" = 5 + 4 = 9 cells.
        let s = "hello世界";
        assert_eq!(display_width(s), 9);
        let result = truncate(s, 8);
        // budget = 5, "hello" fits exactly (5 cells).
        assert_eq!(result, "hello...");
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn truncate_mixed_cjk_then_ascii() {
        // "世界hello" = 4 + 5 = 9 cells.
        let s = "世界hello";
        assert_eq!(display_width(s), 9);
        let result = truncate(s, 8);
        // budget = 5, "世界" (4 cells) + "h" (1 cell) = 5 cells.
        assert_eq!(result, "世界h...");
        assert_eq!(display_width(&result), 8);
    }

    #[test]
    fn truncate_hindi_devanagari() {
        // Devanagari combining sequences. "नमस्ते" has combining characters.
        let s = "नमस्ते";
        let w = display_width(s);
        // Each base consonant is 1 cell, combining marks are 0-width.
        // Should not panic and must fit within the limit.
        let result = truncate(s, 3);
        assert!(
            display_width(&result) <= 3,
            "result={result:?} width={}",
            display_width(&result)
        );

        // When limit is large enough, return as-is.
        assert_eq!(truncate(s, w), s);
    }

    #[test]
    fn truncate_emoji() {
        // Basic emoji: each is 2 cells wide.
        let s = "🚀🔥💡";
        assert_eq!(display_width(s), 6);
        let result = truncate(s, 5);
        // budget = 2, fits 1 emoji (2 cells).
        assert_eq!(result, "🚀...");
        assert_eq!(display_width(&result), 5);
    }

    #[test]
    fn truncate_very_small_width() {
        let s = "hello世界";
        // max_width = 3: just enough for "..." but no chars. Hard-truncate.
        let result = truncate(s, 3);
        assert!(display_width(&result) <= 3);

        // max_width = 2: hard-truncate, no ellipsis.
        let result = truncate(s, 2);
        assert_eq!(result, "he");
        assert!(display_width(&result) <= 2);

        // max_width = 1.
        let result = truncate(s, 1);
        assert_eq!(result, "h");
    }

    #[test]
    fn truncate_zero_width() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn truncate_exact_boundary() {
        // String display width exactly equals max_width — no truncation.
        let s = "abc";
        assert_eq!(truncate(s, 3), "abc");

        let s = "漢字";
        assert_eq!(display_width(s), 4);
        assert_eq!(truncate(s, 4), "漢字");
    }

    // =======================================================================
    // Visual layout tests: verify column alignment with multi-width chars
    // =======================================================================

    /// Simulates a fixed-width column: truncate text, then pad to exactly
    /// `col_width` cells with spaces.
    fn column(text: &str, col_width: usize) -> String {
        let truncated = truncate(text, col_width);
        let w = display_width(&truncated);
        let padding = col_width.saturating_sub(w);
        format!("{truncated}{}", " ".repeat(padding))
    }

    #[test]
    fn visual_column_alignment_mixed() {
        let col_w = 12;
        let rows = [
            column("hello world!", col_w),
            column("漢字テスト長い", col_w),
            column("こんにちは!", col_w),
            column("🚀 launch", col_w),
            column("short", col_w),
        ];

        // Every row must have exactly col_w display width.
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                display_width(row),
                col_w,
                "row {i} ({row:?}) has width {} instead of {col_w}",
                display_width(row),
            );
        }
    }

    #[test]
    fn visual_two_column_table() {
        let col1_w = 10;
        let col2_w = 20;
        let sep = " | ";

        let rows: Vec<String> = [
            ("status", "修正しました"),
            ("file", "src/main.rs"),
            ("author", "田中太郎"),
            ("emoji", "🎉🎊🎈🎁✨💫"),
        ]
        .iter()
        .map(|(c1, c2)| format!("{}{sep}{}", column(c1, col1_w), column(c2, col2_w)))
        .collect();

        let expected_total = col1_w + sep.len() + col2_w;
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                display_width(row),
                expected_total,
                "row {i} ({row:?}) has width {} instead of {expected_total}",
                display_width(row),
            );
        }
    }

    // =======================================================================
    // wrap_str() tests
    // =======================================================================

    #[test]
    fn wrap_ascii() {
        let lines = wrap_str("hello world", 5);
        assert_eq!(lines, vec!["hello", " worl", "d"]);
    }

    #[test]
    fn wrap_cjk() {
        // "漢字テスト" = 10 cells. Wrap at 6 cells.
        let lines = wrap_str("漢字テスト", 6);
        // Each char is 2 cells. 6 / 2 = 3 chars per line.
        assert_eq!(lines, vec!["漢字テ", "スト"]);
        for line in &lines {
            assert!(display_width(line) <= 6, "line {line:?} exceeds width 6");
        }
    }

    #[test]
    fn wrap_cjk_odd_width() {
        // Width 5: a 2-cell CJK char won't fit in the remaining 1 cell.
        // "漢字テスト" → "漢字" (4), "テス" (4), "ト" (2)
        let lines = wrap_str("漢字テスト", 5);
        assert_eq!(lines, vec!["漢字", "テス", "ト"]);
        for line in &lines {
            assert!(display_width(line) <= 5, "line {line:?} exceeds width 5");
        }
    }

    #[test]
    fn wrap_mixed() {
        // "ab漢cd字ef" = 2 + 2 + 2 + 2 + 2 + 2 = 10 cells (wait: a=1, b=1, 漢=2, c=1, d=1, 字=2, e=1, f=1 = 10)
        let s = "ab漢cd字ef";
        assert_eq!(display_width(s), 10);
        let lines = wrap_str(s, 4);
        // "ab漢" = 1+1+2 = 4, "cd字" = 1+1+2 = 4, "ef" = 1+1 = 2
        assert_eq!(lines, vec!["ab漢", "cd字", "ef"]);
    }

    #[test]
    fn wrap_emoji() {
        let s = "🚀🔥💡✨";
        assert_eq!(display_width(s), 8);
        let lines = wrap_str(s, 5);
        // Each emoji is 2 cells. 5 / 2 = 2 per line with 1 cell unused.
        assert_eq!(lines, vec!["🚀🔥", "💡✨"]);
        for line in &lines {
            assert!(display_width(line) <= 5);
        }
    }

    #[test]
    fn wrap_empty() {
        assert_eq!(wrap_str("", 10), vec![""]);
    }

    #[test]
    fn wrap_single_wide_char_exceeding_width() {
        // Width 1: a 2-cell CJK char can't fit, so each char goes on its own line.
        let lines = wrap_str("漢字", 1);
        // The chars are wider than the width — they still get placed one per line
        // since we can't split a character.
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "漢");
        assert_eq!(lines[1], "字");
    }
}
