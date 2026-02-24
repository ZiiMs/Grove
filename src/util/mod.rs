pub mod git;
pub mod pm;

pub use pm::{truncate_with_ellipsis, AuthType, OptionalClient};

pub fn sanitize_branch_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

pub fn sanitize_linear_branch_name(username: &str, identifier: &str, title: &str) -> String {
    let slug = title
        .split_whitespace()
        .map(|word| word.trim_end_matches(['.', '!', '?', ',']))
        .collect::<Vec<_>>()
        .join("-")
        .replace('/', "")
        .to_lowercase();
    let id_lower = identifier.to_lowercase();
    format!("{}/{}-{}", username, id_lower, slug)
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
            "ziim/gre-30-create-helperutils-section-for-reused-code"
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
            "ziim/gre-30-create-helperutils-section-for-reused-code"
        );
    }

    #[test]
    fn test_linear_branch_name_with_multiple_punctuation() {
        assert_eq!(
            sanitize_linear_branch_name("ziim", "GRE-31", "Fix this bug, please?!"),
            "ziim/gre-31-fix-this-bug-please"
        );
    }
}
