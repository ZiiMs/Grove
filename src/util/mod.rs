pub fn sanitize_branch_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

pub fn sanitize_linear_branch_name(username: &str, identifier: &str, title: &str) -> String {
    let slug = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
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
}
