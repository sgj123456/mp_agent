use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOp {
    Write,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub op: PermissionOp,
    pub path_prefix: String,
    pub decision: PermissionDecision,
}

#[derive(Debug)]
pub struct PermissionRequest {
    pub op: PermissionOp,
    pub path: String,
    pub description: String,
}

pub fn needs_permission(
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<(PermissionOp, String)> {
    let r = match tool_name {
        "write_file" | "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str())?;
            Some((PermissionOp::Write, path.to_string()))
        }
        "bash" => {
            let cmd = args.get("command").and_then(|v| v.as_str())?;
            Some((PermissionOp::Execute, format!("bash: {}", cmd)))
        }
        _ => None,
    };
    if let Some((ref op, ref target)) = r {
        tracing::debug!("Permission needed: {:?} on {}", op, target);
    }
    r
}

pub fn op_label(op: &PermissionOp) -> &'static str {
    match op {
        PermissionOp::Write => "WRITE",
        PermissionOp::Execute => "EXEC",
    }
}

pub fn abspath(path: &str) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_string_lossy().to_string()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(p).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

/// Check if any rule matches this op+path. Returns the matching decision if found.
pub fn match_rule(
    rules: &[PermissionRule],
    op: &PermissionOp,
    path: &str,
) -> Option<PermissionDecision> {
    for rule in rules {
        if &rule.op == op && path.starts_with(&rule.path_prefix) {
            return Some(rule.decision.clone());
        }
    }
    None
}

/// Truncate a string for display at a reasonable length.
pub fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        let truncated: String = chars[..max].iter().collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_needs_permission_write_file() {
        let args = json!({"path": "/tmp/test.txt", "content": "hello"});
        let result = needs_permission("write_file", &args).unwrap();
        assert_eq!(result.0, PermissionOp::Write);
        assert_eq!(result.1, "/tmp/test.txt");
    }

    #[test]
    fn test_needs_permission_edit_file() {
        let args = json!({"path": "/tmp/test.txt", "old": "a", "new": "b"});
        let result = needs_permission("edit_file", &args).unwrap();
        assert_eq!(result.0, PermissionOp::Write);
        assert_eq!(result.1, "/tmp/test.txt");
    }

    #[test]
    fn test_needs_permission_bash() {
        let args = json!({"command": "ls -la"});
        let result = needs_permission("bash", &args).unwrap();
        assert_eq!(result.0, PermissionOp::Execute);
        assert_eq!(result.1, "bash: ls -la");
    }

    #[test]
    fn test_needs_permission_unknown_tool() {
        let args = json!({"path": "/tmp/test.txt"});
        let result = needs_permission("unknown_tool", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_needs_permission_missing_path() {
        let args = json!({});
        let result = needs_permission("write_file", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_needs_permission_missing_command() {
        let args = json!({});
        let result = needs_permission("bash", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_op_label() {
        assert_eq!(op_label(&PermissionOp::Write), "WRITE");
        assert_eq!(op_label(&PermissionOp::Execute), "EXEC");
    }

    #[test]
    fn test_match_rule_allow() {
        let rules = vec![PermissionRule {
            op: PermissionOp::Write,
            path_prefix: "/tmp".into(),
            decision: PermissionDecision::Allow,
        }];
        let result = match_rule(&rules, &PermissionOp::Write, "/tmp/test.txt").unwrap();
        assert_eq!(result, PermissionDecision::Allow);
    }

    #[test]
    fn test_match_rule_deny() {
        let rules = vec![PermissionRule {
            op: PermissionOp::Write,
            path_prefix: "/etc".into(),
            decision: PermissionDecision::Deny,
        }];
        let result = match_rule(&rules, &PermissionOp::Write, "/etc/passwd").unwrap();
        assert_eq!(result, PermissionDecision::Deny);
    }

    #[test]
    fn test_match_rule_no_match_different_op() {
        let rules = vec![PermissionRule {
            op: PermissionOp::Write,
            path_prefix: "/tmp".into(),
            decision: PermissionDecision::Allow,
        }];
        let result = match_rule(&rules, &PermissionOp::Execute, "/tmp/test.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_match_rule_no_match_different_path() {
        let rules = vec![PermissionRule {
            op: PermissionOp::Write,
            path_prefix: "/tmp".into(),
            decision: PermissionDecision::Allow,
        }];
        let result = match_rule(&rules, &PermissionOp::Write, "/etc/passwd");
        assert!(result.is_none());
    }

    #[test]
    fn test_match_rule_empty_rules() {
        let rules: Vec<PermissionRule> = vec![];
        let result = match_rule(&rules, &PermissionOp::Write, "/tmp/test.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_truncate_short_string() {
        let result = truncate("hello", 10);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = truncate("hello world this is a long string", 5);
        assert_eq!(result, "hello...");
        assert_eq!(result.len(), 8); // 5 chars + "..."
    }

    #[test]
    fn test_truncate_exact_boundary() {
        let result = truncate("hello", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_abspath_absolute() {
        let result = abspath("/tmp/test.txt");
        assert_eq!(result, "/tmp/test.txt");
    }

    #[test]
    fn test_abspath_relative() {
        let result = abspath("tmp/test.txt");
        let cwd = std::env::current_dir().unwrap();
        let expected = cwd.join("tmp/test.txt").to_string_lossy().to_string();
        assert_eq!(result, expected);
    }
}
