const MAX_BRANCH_LENGTH: usize = 50;

fn truncate_to_words(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let words: Vec<&str> = s.split('-').collect();
    let mut result = String::new();
    for word in words {
        let new_len = if result.is_empty() {
            word.len()
        } else {
            result.len() + 1 + word.len()
        };
        if new_len > max_len {
            break;
        }
        if !result.is_empty() {
            result.push('-');
        }
        result.push_str(word);
    }
    result
}

pub fn sanitize_branch_name(name: &str) -> String {
    let sanitized = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    truncate_to_words(&sanitized, MAX_BRANCH_LENGTH)
}

pub fn sanitize_linear_branch_name(username: &str, identifier: &str, title: &str) -> String {
    let id_lower = identifier.to_lowercase();
    let prefix_len = username.len() + 1 + id_lower.len() + 1;
    let max_slug_len = MAX_BRANCH_LENGTH.saturating_sub(prefix_len);
    let slug = title
        .split_whitespace()
        .map(|word| word.trim_end_matches(['.', '!', '?', ',']))
        .collect::<Vec<_>>()
        .join("-")
        .replace('/', "")
        .to_lowercase();
    let truncated_slug = truncate_to_words(&slug, max_slug_len);
    format!("{}/{}-{}", username, id_lower, truncated_slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_branch_name("space in name"), "space-in-name");
    }

    #[test]
    fn test_sanitize_multiple_spaces() {
        assert_eq!(sanitize_branch_name("  space  in  name  "), "space-in-name");
    }

    #[test]
    fn test_sanitize_uppercase() {
        assert_eq!(sanitize_branch_name("Space In Name"), "space-in-name");
    }

    #[test]
    fn test_sanitize_single_word() {
        assert_eq!(sanitize_branch_name("single"), "single");
    }

    #[test]
    fn test_sanitize_only_spaces() {
        assert_eq!(sanitize_branch_name("   "), "");
    }

    #[test]
    fn test_linear_branch_name() {
        assert_eq!(
            sanitize_linear_branch_name("ziim", "GRE-23", "Linear Git Integration"),
            "ziim/gre-23-linear-git-integration"
        );
    }

    #[test]
    fn test_linear_branch_name_mixed_case() {
        assert_eq!(
            sanitize_linear_branch_name("JohnDoe", "ABC-456", "Some Task Name HERE"),
            "JohnDoe/abc-456-some-task-name-here"
        );
    }

    #[test]
    fn test_linear_branch_name_with_slash() {
        assert_eq!(
            sanitize_linear_branch_name(
                "ziim",
                "GRE-30",
                "Create helper/utils section for reused code"
            ),
            "ziim/gre-30-create-helperutils-section-for-reused"
        );
    }

    #[test]
    fn test_linear_branch_name_with_trailing_punctuation() {
        assert_eq!(
            sanitize_linear_branch_name(
                "ziim",
                "GRE-30",
                "Create helper/utils section for reused code."
            ),
            "ziim/gre-30-create-helperutils-section-for-reused"
        );
    }

    #[test]
    fn test_linear_branch_name_with_multiple_punctuation() {
        assert_eq!(
            sanitize_linear_branch_name("ziim", "GRE-31", "Fix this bug, please?!"),
            "ziim/gre-31-fix-this-bug-please"
        );
    }

    #[test]
    fn test_truncate_long_branch_name() {
        let long_name =
            "Fix the bug that causes really long branch names to break everything in the system";
        let result = sanitize_branch_name(long_name);
        assert!(result.len() <= 50);
        assert_eq!(result, "fix-the-bug-that-causes-really-long-branch-names");
    }

    #[test]
    fn test_truncate_preserves_short_name() {
        assert_eq!(sanitize_branch_name("short name"), "short-name");
    }

    #[test]
    fn test_truncate_fits_within_limit() {
        let input = "fix the bug that causes really long branch name";
        let result = sanitize_branch_name(input);
        assert!(result.len() <= 50);
        assert_eq!(result, "fix-the-bug-that-causes-really-long-branch-name");
    }

    #[test]
    fn test_linear_truncate_long_title() {
        let result = sanitize_linear_branch_name(
            "ziim",
            "GRE-32",
            "Truncate branch name to fix crazy long names from tasks that are way too long",
        );
        assert!(result.len() <= 50);
        assert!(result.starts_with("ziim/gre-32-"));
    }

    #[test]
    fn test_linear_preserves_identifier() {
        let result = sanitize_linear_branch_name(
            "user",
            "ABC-123",
            "This is a very long task title that needs to be truncated properly",
        );
        assert!(result.starts_with("user/abc-123-"));
        assert!(result.len() <= 50);
    }
}
