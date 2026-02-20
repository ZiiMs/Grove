pub fn sanitize_branch_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
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
}
