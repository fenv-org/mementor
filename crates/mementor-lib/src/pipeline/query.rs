use crate::config::MIN_QUERY_UNITS;

/// Classification result for a user prompt.
///
/// Used to decide whether to perform embedding + vector search (`Searchable`)
/// or skip recall entirely (`Trivial`).
#[derive(Debug, PartialEq, Eq)]
pub enum QueryClass {
    /// The prompt carries enough semantic content for useful recall.
    Searchable,
    /// The prompt is trivial and should skip recall entirely.
    Trivial {
        /// Human-readable reason for the classification.
        reason: &'static str,
    },
}

/// Classify a user prompt for recall worthiness.
///
/// Returns `Searchable` if the prompt is worth embedding and searching,
/// or `Trivial` with a reason if recall should be skipped.
///
/// Rules applied in order:
/// 1. Slash commands: any token starting with `/` (without subsequent `/`)
/// 2. Information units: fewer than [`MIN_QUERY_UNITS`] language-adaptive units
pub fn classify_query(prompt: &str) -> QueryClass {
    let trimmed = prompt.trim();

    if has_slash_command(trimmed) {
        return QueryClass::Trivial {
            reason: "slash command",
        };
    }

    if count_information_units(trimmed) < MIN_QUERY_UNITS {
        return QueryClass::Trivial {
            reason: "too short",
        };
    }

    QueryClass::Searchable
}

/// Check whether the text contains a slash command token.
///
/// A slash command is a whitespace-delimited token that starts with `/` and
/// contains no subsequent `/` (to distinguish from file paths like
/// `/tmp/test.txt`).
fn has_slash_command(text: &str) -> bool {
    text.split_whitespace()
        .any(|token| token.starts_with('/') && token.len() > 1 && !token[1..].contains('/'))
}

/// Check whether a character is logographic (each character carries
/// independent meaning and counts as one information unit).
///
/// Covers CJK ideographs, hiragana, katakana (full-width and half-width).
/// Korean (Hangul) is NOT included because Korean uses spaces between words.
const fn is_logographic(ch: char) -> bool {
    matches!(ch,
        '\u{3040}'..='\u{309F}'   // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana (full-width)
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{FF65}'..='\u{FF9F}' // Half-width Katakana
    )
}

/// Count language-adaptive information units in text.
///
/// - Each logographic character (CJK ideograph, kana) counts as 1 unit.
/// - Each whitespace-separated group of other characters counts as 1 unit.
///
/// This handles Chinese/Japanese (no spaces) and Latin/Korean/Cyrillic
/// (space-separated) uniformly with a single threshold.
fn count_information_units(text: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            in_word = false;
        } else if is_logographic(ch) {
            count += 1;
            in_word = false;
        } else if !in_word {
            count += 1;
            in_word = true;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- classify_query tests ---

    #[test]
    fn classify_slash_commands() {
        assert_eq!(
            classify_query("/commit"),
            QueryClass::Trivial {
                reason: "slash command"
            }
        );
        assert_eq!(
            classify_query("  /worktree add feature"),
            QueryClass::Trivial {
                reason: "slash command"
            }
        );
        assert_eq!(
            classify_query("ok /commit"),
            QueryClass::Trivial {
                reason: "slash command"
            }
        );
        assert_eq!(
            classify_query("please /review-pr 123"),
            QueryClass::Trivial {
                reason: "slash command"
            }
        );
    }

    #[test]
    fn classify_file_paths_not_slash_commands() {
        // File paths have `/` after the initial one — not slash commands.
        // These are 3+ units so they're Searchable.
        assert_eq!(
            classify_query("read /tmp/test.txt please"),
            QueryClass::Searchable
        );
        assert_eq!(
            classify_query("look at /Users/foo/bar.rs"),
            QueryClass::Searchable
        );
    }

    #[test]
    fn classify_short_prompts() {
        assert_eq!(
            classify_query("push"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        assert_eq!(
            classify_query("check ci"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        assert_eq!(
            classify_query("ok"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
    }

    #[test]
    fn classify_searchable_prompts() {
        assert_eq!(
            classify_query("How do I implement authentication in Rust?"),
            QueryClass::Searchable
        );
        assert_eq!(
            classify_query("explain the ingest pipeline"),
            QueryClass::Searchable
        );
        assert_eq!(classify_query("fix the bug"), QueryClass::Searchable);
    }

    #[test]
    fn classify_whitespace_handling() {
        // Leading/trailing whitespace is trimmed
        assert_eq!(
            classify_query("  /commit  "),
            QueryClass::Trivial {
                reason: "slash command"
            }
        );
        assert_eq!(
            classify_query("  push  "),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        // Three words with extra internal whitespace is still searchable
        assert_eq!(classify_query("  fix  the  bug  "), QueryClass::Searchable);
    }

    #[test]
    fn classify_cjk_prompts() {
        // Chinese: 6 units (each ideograph = 1 unit)
        assert_eq!(classify_query("修改配置文件吧"), QueryClass::Searchable);
        // Chinese: 2 units — too short
        assert_eq!(
            classify_query("推送"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        // Japanese with hiragana: 5 units
        assert_eq!(classify_query("設定を修正"), QueryClass::Searchable);
        // Half-width katakana: 3 units
        assert_eq!(classify_query("ｶﾀｶﾅ"), QueryClass::Searchable);
        // Half-width katakana: 2 units — too short
        assert_eq!(
            classify_query("ｶﾅ"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
    }

    #[test]
    fn classify_korean_prompts() {
        // Korean uses spaces — counted as words
        // 2 words: too short
        assert_eq!(
            classify_query("코드를 수정해줘"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        // 3 words: searchable
        assert_eq!(classify_query("이 코드를 수정해줘"), QueryClass::Searchable);
    }

    #[test]
    fn classify_mixed_script() {
        // "修改 config" = 2 CJK + 1 Latin = 3 units
        assert_eq!(classify_query("修改 config"), QueryClass::Searchable);
        // "修改config" = 2 CJK + 1 Latin = 3 units (no space needed between scripts)
        assert_eq!(classify_query("修改config"), QueryClass::Searchable);
    }

    #[test]
    fn classify_edge_cases() {
        // Exactly 3 units: searchable
        assert_eq!(classify_query("fix the bug"), QueryClass::Searchable);
        // Exactly 2 units: too short
        assert_eq!(
            classify_query("fix bug"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        // Empty string: too short (0 units)
        assert_eq!(
            classify_query(""),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        // Only whitespace: too short (0 units after trim)
        assert_eq!(
            classify_query("   "),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
        // Single `/` is not a slash command (len must be > 1)
        assert_eq!(
            classify_query("/"),
            QueryClass::Trivial {
                reason: "too short"
            }
        );
    }

    // --- count_information_units tests ---

    #[test]
    fn count_units_english() {
        assert_eq!(count_information_units("push"), 1);
        assert_eq!(count_information_units("check ci"), 2);
        assert_eq!(count_information_units("fix the bug"), 3);
        assert_eq!(count_information_units("  fix  the  bug  "), 3);
    }

    #[test]
    fn count_units_cjk() {
        assert_eq!(count_information_units("推送"), 2);
        assert_eq!(count_information_units("修改配置文件"), 6);
        assert_eq!(count_information_units("設定を修正"), 5);
    }

    #[test]
    fn count_units_halfwidth_katakana() {
        assert_eq!(count_information_units("ｶﾀｶﾅ"), 4);
        assert_eq!(count_information_units("ｶﾅ"), 2);
    }

    #[test]
    fn count_units_korean() {
        assert_eq!(count_information_units("코드를 수정해줘"), 2);
        assert_eq!(count_information_units("이 코드를 수정해줘"), 3);
    }

    #[test]
    fn count_units_mixed() {
        // "修改 config" = 2 CJK + 1 Latin = 3
        assert_eq!(count_information_units("修改 config"), 3);
        // "修改config" = 2 CJK + 1 Latin = 3 (logographic breaks word state)
        assert_eq!(count_information_units("修改config"), 3);
        // "config修改" = 1 Latin + 2 CJK = 3
        assert_eq!(count_information_units("config修改"), 3);
    }

    #[test]
    fn count_units_empty() {
        assert_eq!(count_information_units(""), 0);
        assert_eq!(count_information_units("   "), 0);
    }

    // --- has_slash_command tests ---

    #[test]
    fn slash_command_detection() {
        assert!(has_slash_command("/commit"));
        assert!(has_slash_command("ok /commit"));
        assert!(has_slash_command("please /review-pr 123"));
        assert!(has_slash_command("  /worktree"));
    }

    #[test]
    fn file_paths_not_slash_commands() {
        assert!(!has_slash_command("/tmp/test.txt"));
        assert!(!has_slash_command("/Users/foo/bar.rs"));
        assert!(!has_slash_command("read /etc/hosts"));
        // Single "/" is not a slash command
        assert!(!has_slash_command("/"));
        // No tokens start with "/"
        assert!(!has_slash_command("hello world"));
        assert!(!has_slash_command(""));
    }
}
