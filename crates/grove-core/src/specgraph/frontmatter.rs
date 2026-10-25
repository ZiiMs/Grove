//! A deliberately small YAML frontmatter reader.
//!
//! Spec files carry a handful of fields — `id`, `type`, `parent`, `depends-on`, `references`,
//! `implements`, `covers`, `title`, `status` — and nothing else is read. Rather than take a YAML
//! dependency (and the anchor/alias/tag surface that comes with it), this parser understands exactly
//! the subset spec frontmatter uses:
//!
//! - `key: scalar`, optionally single- or double-quoted
//! - `key: [a, "b c", 'd']` inline lists
//! - block lists:
//!   ```text
//!   depends-on:
//!     - one
//!     - "two"
//!   ```
//! - full-line `#` comments, blank lines, CRLF and a leading BOM
//!
//! Values may contain `:` (only the first one separates key from value). The raw frontmatter block is
//! kept verbatim so [`crate::specgraph::write_spec`] can replace the body without reformatting the
//! author's frontmatter.

use std::collections::BTreeMap;

/// One frontmatter value. Scalars and lists stay distinct so an empty `parent:` is absent rather than
/// a one-element list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontmatterValue {
    Scalar(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrontmatterError {
    #[error("missing YAML frontmatter block")]
    Missing,
    #[error("unterminated YAML frontmatter block")]
    Unterminated,
    #[error("invalid frontmatter at line {line}: {reason}")]
    Invalid { line: usize, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    fields: BTreeMap<String, FrontmatterValue>,
    /// Opening `---` line through the closing delimiter, byte-for-byte.
    raw: String,
    /// Everything after the closing delimiter.
    body: String,
}

impl Frontmatter {
    /// Parse a document. Files without a frontmatter block are not spec nodes, which is why a missing
    /// block is an error rather than an empty result.
    pub fn parse(source: &str) -> Result<Self, FrontmatterError> {
        let (opening_end, opening) = line_at(source, 0).ok_or(FrontmatterError::Missing)?;
        let opening = opening.trim_start_matches('\u{feff}').trim();
        if opening != "---" {
            return Err(FrontmatterError::Missing);
        }

        let mut fields = BTreeMap::new();
        let mut pending_list: Option<(String, Vec<String>)> = None;
        let mut offset = opening_end;
        let mut line_number = 1;

        while let Some((line_end, line)) = line_at(source, offset) {
            line_number += 1;
            offset = line_end;
            let trimmed = line.trim();

            if trimmed == "---" || trimmed == "..." {
                flush_list(&mut fields, &mut pending_list);
                return Ok(Self {
                    fields,
                    raw: source[..line_end].to_string(),
                    body: source[line_end..].to_string(),
                });
            }

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some((_, items)) = pending_list.as_mut() {
                if let Some(item) = trimmed.strip_prefix('-') {
                    if item.is_empty() || item.starts_with(char::is_whitespace) {
                        let token = strip_comment(item);
                        if !token.is_empty() {
                            items.push(parse_scalar(token).map_err(|reason| {
                                FrontmatterError::Invalid {
                                    line: line_number,
                                    reason,
                                }
                            })?);
                        }
                        continue;
                    }
                }
                flush_list(&mut fields, &mut pending_list);
            }

            let Some((key, value)) = trimmed.split_once(':') else {
                return Err(FrontmatterError::Invalid {
                    line: line_number,
                    reason: format!("expected `key: value`, found `{trimmed}`"),
                });
            };
            let key = key.trim();
            if key.is_empty() {
                return Err(FrontmatterError::Invalid {
                    line: line_number,
                    reason: "empty key".to_string(),
                });
            }

            let value = strip_comment(value).trim();
            if value.is_empty() {
                pending_list = Some((key.to_string(), Vec::new()));
            } else if let Some(inner) = value.strip_prefix('[') {
                let Some(inner) = inner.strip_suffix(']') else {
                    return Err(FrontmatterError::Invalid {
                        line: line_number,
                        reason: "inline list is missing its closing `]`".to_string(),
                    });
                };
                let items =
                    split_inline_list(inner).map_err(|reason| FrontmatterError::Invalid {
                        line: line_number,
                        reason,
                    })?;
                fields.insert(key.to_string(), FrontmatterValue::List(items));
            } else {
                let scalar = parse_scalar(value).map_err(|reason| FrontmatterError::Invalid {
                    line: line_number,
                    reason,
                })?;
                fields.insert(key.to_string(), FrontmatterValue::Scalar(scalar));
            }
        }

        Err(FrontmatterError::Unterminated)
    }

    /// A non-empty scalar. `parent:` with no value reads as `None`.
    pub fn scalar(&self, key: &str) -> Option<&str> {
        match self.fields.get(key) {
            Some(FrontmatterValue::Scalar(value)) if !value.is_empty() => Some(value),
            _ => None,
        }
    }

    /// A list field. A scalar value is coerced to a one-item list, so both `depends-on: x` and
    /// `depends-on: [x]` mean the same thing to the graph.
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.fields.get(key) {
            Some(FrontmatterValue::List(items)) => items.clone(),
            Some(FrontmatterValue::Scalar(value)) if !value.is_empty() => vec![value.clone()],
            _ => Vec::new(),
        }
    }

    /// The raw frontmatter block, delimiters included.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn body(&self) -> &str {
        &self.body
    }
}

fn flush_list(
    fields: &mut BTreeMap<String, FrontmatterValue>,
    pending: &mut Option<(String, Vec<String>)>,
) {
    if let Some((key, items)) = pending.take() {
        fields.insert(key, FrontmatterValue::List(items));
    }
}

/// Returns the byte offset just past the line ending, plus the line including its terminator.
fn line_at(source: &str, offset: usize) -> Option<(usize, &str)> {
    if offset >= source.len() {
        return None;
    }
    let end = source[offset..]
        .find('\n')
        .map(|index| offset + index + 1)
        .unwrap_or(source.len());
    Some((end, &source[offset..end]))
}

/// Drop an unquoted trailing `# comment`. A `#` inside quotes or brackets is data.
fn strip_comment(value: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut previous_was_space = true;

    for (index, character) in value.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            previous_was_space = character.is_whitespace();
            continue;
        }

        match character {
            '"' | '\'' => quote = Some(character),
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '#' if depth == 0 && previous_was_space => return value[..index].trim_end(),
            _ => {}
        }
        previous_was_space = character.is_whitespace();
    }

    value.trim_end()
}

/// Parse a single quoted or plain scalar. Quotes must wrap the whole token.
fn parse_scalar(token: &str) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(String::new());
    }

    if let Some(rest) = token.strip_prefix('"') {
        let mut result = String::new();
        let mut characters = rest.chars();
        loop {
            match characters.next() {
                None => return Err("unterminated double-quoted string".to_string()),
                Some('"') => {
                    if characters.next().is_some() {
                        return Err("content after a closing double quote".to_string());
                    }
                    return Ok(result);
                }
                Some('\\') => match characters.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some(other) => result.push(other),
                    None => return Err("dangling escape in double-quoted string".to_string()),
                },
                Some(character) => result.push(character),
            }
        }
    }

    if let Some(rest) = token.strip_prefix('\'') {
        let mut result = String::new();
        let mut characters = rest.chars().peekable();
        loop {
            match characters.next() {
                None => return Err("unterminated single-quoted string".to_string()),
                Some('\'') if characters.peek() == Some(&'\'') => {
                    characters.next();
                    result.push('\'');
                }
                Some('\'') => {
                    if characters.next().is_some() {
                        return Err("content after a closing single quote".to_string());
                    }
                    return Ok(result);
                }
                Some(character) => result.push(character),
            }
        }
    }

    Ok(token.to_string())
}

/// Split `a, "b, c", d` on the commas that are not inside quotes.
fn split_inline_list(inner: &str) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for character in inner.chars() {
        if let Some(active) = quote {
            current.push(character);
            if escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }

        match character {
            '"' | '\'' => {
                quote = Some(character);
                current.push(character);
            }
            ',' => {
                push_inline_item(&mut items, &current)?;
                current.clear();
            }
            _ => current.push(character),
        }
    }

    if quote.is_some() {
        return Err("unterminated quote in inline list".to_string());
    }
    push_inline_item(&mut items, &current)?;
    Ok(items)
}

fn push_inline_item(items: &mut Vec<String>, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(());
    }
    items.push(parse_scalar(token)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Frontmatter {
        Frontmatter::parse(source).expect("frontmatter parses")
    }

    #[test]
    fn reads_scalars_and_preserves_body_bytes() {
        let source = "---\nid: grove-goal\ntype: goal-and-requirements\ntitle: Grove: the app\nstatus: active\n---\n# Body\n\nText.\n";
        let frontmatter = parse(source);
        assert_eq!(frontmatter.scalar("id"), Some("grove-goal"));
        assert_eq!(frontmatter.scalar("type"), Some("goal-and-requirements"));
        assert_eq!(frontmatter.scalar("title"), Some("Grove: the app"));
        assert_eq!(frontmatter.scalar("status"), Some("active"));
        assert_eq!(frontmatter.body(), "# Body\n\nText.\n");
        assert_eq!(
            frontmatter.raw(),
            "---\nid: grove-goal\ntype: goal-and-requirements\ntitle: Grove: the app\nstatus: active\n---\n"
        );
    }

    #[test]
    fn reads_inline_quoted_lists() {
        let frontmatter = parse(
            "---\nid: a\ntype: t\ndepends-on: [\"one\", 'two words', three]\nreferences: []\n---\n",
        );
        assert_eq!(
            frontmatter.list("depends-on"),
            vec!["one", "two words", "three"]
        );
        assert!(frontmatter.list("references").is_empty());
    }

    #[test]
    fn reads_block_lists_and_comments() {
        let frontmatter = parse(
            "---\n# leading comment\nid: a\ntype: t\ndepends-on:\n  - one\n  - \"two\"\n  # a comment\n  - 'three'\nreferences:\n  - x # trailing comment\n---\n",
        );
        assert_eq!(frontmatter.list("depends-on"), vec!["one", "two", "three"]);
        assert_eq!(frontmatter.list("references"), vec!["x"]);
    }

    #[test]
    fn coerces_a_scalar_into_a_one_item_list() {
        let frontmatter = parse("---\nid: a\ntype: t\nparent: root\n---\n");
        assert_eq!(frontmatter.list("parent"), vec!["root"]);
        assert_eq!(frontmatter.scalar("parent"), Some("root"));
    }

    #[test]
    fn handles_crlf_and_a_bom() {
        let frontmatter = parse("\u{feff}---\r\nid: a\r\ntype: t\r\n---\r\n# Body\r\n");
        assert_eq!(frontmatter.scalar("id"), Some("a"));
        assert_eq!(frontmatter.scalar("type"), Some("t"));
        assert_eq!(frontmatter.body(), "# Body\r\n");
    }

    #[test]
    fn hashes_and_colons_inside_quotes_survive_comment_stripping() {
        let frontmatter = parse(
            "---\nid: a\ntype: t\ntitle: \"has # hash and: colon\"\nreferences: [\"a#b\"]\n---\n",
        );
        assert_eq!(frontmatter.scalar("title"), Some("has # hash and: colon"));
        assert_eq!(frontmatter.list("references"), vec!["a#b"]);
    }

    #[test]
    fn a_missing_or_unterminated_block_is_an_error() {
        assert_eq!(
            Frontmatter::parse("# just markdown\n").unwrap_err(),
            FrontmatterError::Missing
        );
        assert_eq!(
            Frontmatter::parse("---\nid: a\ntype: t\n").unwrap_err(),
            FrontmatterError::Unterminated
        );
    }

    #[test]
    fn a_line_without_a_colon_is_an_error() {
        let error = Frontmatter::parse("---\nnot a key value pair\n---\n").unwrap_err();
        assert!(matches!(error, FrontmatterError::Invalid { line: 2, .. }));
    }

    #[test]
    fn unterminated_quotes_are_errors() {
        assert!(Frontmatter::parse("---\nid: \"a\ntype: t\n---\n").is_err());
        assert!(Frontmatter::parse("---\nid: a\ntype: t\ndepends-on: [\"a]\n---\n").is_err());
    }

    #[test]
    fn empty_scalar_reads_as_absent() {
        let frontmatter = parse("---\nid: a\ntype: t\nparent:\n---\n");
        assert_eq!(frontmatter.scalar("parent"), None);
        assert!(frontmatter.list("parent").is_empty());
    }
}
