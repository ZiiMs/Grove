//! The prompt and the cleanup for a model-written session title.
//!
//! A title completion is a one-shot request, but the model still answers in prose: quotes, a
//! `Title:` prefix, trailing punctuation and long lines are all normal. The sanitizer is the single
//! place that turns that answer into the string the tree and the header can show, and it rejects an
//! empty answer rather than storing one.

/// The instruction the title model sees. It asks for the conversation's own language: a user who
/// writes in French gets a French title, not an English one.
pub const TITLE_SYSTEM_PROMPT: &str = "You name conversations. Reply with only a short title of 3–6 words, in the language of the conversation. No quotes, no punctuation at the end.";

/// How much of each turn travels with the title request. The first exchange is enough to tell what a
/// conversation is about; a whole transcript would cost more than the title is worth.
pub const TITLE_PROMPT_MAX_CHARS: usize = 2_000;

/// The output budget. A title is a handful of words; this leaves room for a chatty model without
/// letting it answer with a paragraph.
pub const TITLE_MAX_TOKENS: u32 = 64;

/// The longest title the tree row and the header can show.
pub const TITLE_MAX_CHARS: usize = 60;

/// The user content for the title request: the first exchange, each side capped on a character
/// boundary.
pub fn title_prompt(user: &str, assistant: &str) -> String {
    format!(
        "First user message:\n{}\n\nFirst assistant reply:\n{}\n\nGive this conversation a title.",
        truncate(user, TITLE_PROMPT_MAX_CHARS),
        truncate(assistant, TITLE_PROMPT_MAX_CHARS),
    )
}

/// Turn one answer from the model into a title, or reject it.
///
/// The order matters: a model often wraps the whole title in backticks *after* a `Title:` prefix, so
/// the unwrapping runs again once the prefix is gone. Everything after that is presentation the tree
/// would otherwise show verbatim: collapsed whitespace, no trailing sentence punctuation, and a hard
/// character cap.
pub fn sanitize_title(raw: &str) -> Option<String> {
    let mut title = raw.trim();
    title = unwrap(title);
    title = drop_title_prefix(title);
    title = unwrap(title);
    let collapsed = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim_end_matches(['.', ':'])
        .trim_end()
        .to_string();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate(&trimmed, TITLE_MAX_CHARS).trim_end().to_string())
}

/// Strip one matching pair of surrounding quotes or backticks.
fn unwrap(value: &str) -> &str {
    let trimmed = value.trim();
    for quote in ['"', '\'', '`'] {
        if let Some(rest) = trimmed.strip_prefix(quote) {
            if let Some(inner) = rest.strip_suffix(quote) {
                return inner.trim();
            }
        }
    }
    trimmed
}

/// Drop a leading `Title:` (any case) and the space after it.
fn drop_title_prefix(value: &str) -> &str {
    let lower = value.to_ascii_lowercase();
    match lower.strip_prefix("title:") {
        Some(_) => value["title:".len()..].trim(),
        None => value,
    }
}

/// Take at most `max` characters without splitting one.
fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_and_unwrapped() {
        let cases = [
            ("  Fix the login bug.  ", "Fix the login bug"),
            ("\"Fix the login bug.\"", "Fix the login bug"),
            ("'Fix the login bug'", "Fix the login bug"),
            ("`Refactor the session store`", "Refactor the session store"),
            ("Title: Add OAuth device flow", "Add OAuth device flow"),
            ("title: Add OAuth device flow", "Add OAuth device flow"),
            ("Title: `Wire up the picker`", "Wire up the picker"),
            ("Fix    the\n login bug", "Fix the login bug"),
            ("Ship it:", "Ship it"),
            ("Ship it...", "Ship it"),
        ];
        for (raw, expected) in cases {
            assert_eq!(sanitize_title(raw).as_deref(), Some(expected), "{raw:?}");
        }
    }

    #[test]
    fn an_empty_or_punctuation_only_answer_is_rejected() {
        for raw in [
            "", "   ", "\"\"", "``", "Title:", "Title: .", "...", ":", "\"  \"",
        ] {
            assert_eq!(sanitize_title(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn a_title_is_capped_at_sixty_characters() {
        let long = "a".repeat(80);
        let title = sanitize_title(&long).expect("a long title is still a title");
        assert_eq!(title.chars().count(), TITLE_MAX_CHARS);

        let exact = "b".repeat(TITLE_MAX_CHARS);
        assert_eq!(sanitize_title(&exact).as_deref(), Some(exact.as_str()));
    }

    #[test]
    fn the_prompt_carries_both_turns_and_truncates_each_side() {
        let user = "u".repeat(TITLE_PROMPT_MAX_CHARS + 100);
        let assistant = "a".repeat(TITLE_PROMPT_MAX_CHARS + 100);
        let prompt = title_prompt(&user, &assistant);
        assert!(prompt.contains(&"u".repeat(TITLE_PROMPT_MAX_CHARS)));
        assert!(prompt.contains(&"a".repeat(TITLE_PROMPT_MAX_CHARS)));
        assert!(!prompt.contains(&"u".repeat(TITLE_PROMPT_MAX_CHARS + 1)));
        assert!(!prompt.contains(&"a".repeat(TITLE_PROMPT_MAX_CHARS + 1)));
    }
}
