use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A skill's metadata — loaded eagerly and exposed in the system prompt so the
/// model knows what skills exist and when to trigger them.
///
/// The full skill body is **not** injected into the system prompt; it is loaded
/// on-demand via `read_file` (or `load_skill`) when the model decides a skill
/// is relevant. This keeps prompt size bounded and makes skill loading
/// "disclosive" (the model sees what's available, then pulls what it needs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub path: PathBuf,
}

impl Skill {
    /// Parse a skill file.
    ///
    /// Supports two formats:
    ///
    /// **Format 1 — YAML frontmatter** (recommended):
    /// ```yaml
    /// ---
    /// name: my-skill
    /// description: What this skill does.
    /// license: MIT
    /// compatibility: Requires Python 3
    /// metadata:
    ///   author: me
    ///   version: "1.0"
    /// allowed-tools: Bash(python3:*)
    /// ---
    /// # Skill body...
    ///
    /// ## Triggers
    /// - trigger1
    /// - trigger2
    /// ```
    ///
    /// **Format 2 — line-based** (legacy fallback):
    /// - Line 1: skill name (after optional `# ` prefix)
    /// - Line 2+: description, followed by optional `## Triggers:` section
    /// - Everything before `## Triggers:` is the description.
    /// - Lines under `## Triggers:` (one per line) are trigger keywords.
    pub fn load_from_file(path: &Path) -> color_eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;

        // Try YAML frontmatter first
        if content.starts_with("---") {
            return Self::from_frontmatter(&content, path);
        }

        // Legacy line-based format
        Self::from_legacy_format(&content, path)
    }

    fn from_frontmatter(content: &str, path: &Path) -> color_eyre::Result<Self> {
        let (frontmatter, body) = parse_frontmatter(content);
        let fm = parse_yaml_kv(&frontmatter);

        let name = fm
            .get("name")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });

        let description = fm
            .get("description")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default();

        let triggers = parse_triggers_from_body(body);

        Ok(Skill {
            name,
            description,
            triggers,
            path: path.to_path_buf(),
        })
    }

    fn from_legacy_format(content: &str, path: &Path) -> color_eyre::Result<Self> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut lines = content.lines().map(|s| s.trim()).filter(|s| !s.is_empty());

        let first = lines.next().unwrap_or("");
        let skill_name = first
            .strip_prefix("# ")
            .or(first.strip_prefix("## "))
            .unwrap_or(first)
            .trim()
            .to_string();

        let mut description_lines = Vec::new();
        let mut triggers = Vec::new();
        let mut in_triggers = false;

        for line in lines {
            if line.starts_with("## Triggers:") || line.starts_with("## triggers:") {
                in_triggers = true;
                continue;
            }
            if line.starts_with("## ") {
                in_triggers = false;
                continue;
            }
            if in_triggers {
                let t = line.trim().trim_start_matches('-').trim().to_string();
                if !t.is_empty() {
                    triggers.push(t);
                }
            } else {
                description_lines.push(line);
            }
        }

        let description = description_lines.join("\n").trim().to_string();

        Ok(Skill {
            name: if skill_name.is_empty() {
                name
            } else {
                skill_name
            },
            description,
            triggers,
            path: path.to_path_buf(),
        })
    }
}

/// Split content into (frontmatter_text, body_text).
/// Assumes content starts with `---\n`.
fn parse_frontmatter(content: &str) -> (String, &str) {
    if !content.starts_with("---") {
        return ("".to_string(), content.trim());
    }

    // Content after the opening `---`
    let rest = &content[3..];
    // Strip leading newline if present
    let rest = rest.strip_prefix('\n').unwrap_or(rest);

    // Find the closing `---`
    if let Some(pos) = rest.find("\n---") {
        let front = &rest[..pos];
        let body = &rest[pos + 4..];
        (front.to_string(), body.trim())
    } else {
        // No closing `---` found; entire rest is body
        ("".to_string(), rest.trim())
    }
}

/// Parse simple YAML key-value pairs (no arrays, no nested objects except single-level).
fn parse_yaml_kv(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current_key: Option<String> = None;
    let mut in_multiline = false;
    let mut multiline_parts: Vec<String> = Vec::new();

    for line in text.lines() {
        if let Some(stripped) = line.strip_prefix('#') {
            // Comment: skip
            // But check if it continues a folded value
            if in_multiline {
                multiline_parts.push(stripped.trim().to_string());
            }
            continue;
        }

        if in_multiline {
            // Check if this line is indented (continuation of folded value)
            if line.starts_with(' ') || line.starts_with('\t') || line.is_empty() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    multiline_parts.push(trimmed.to_string());
                }
                continue;
            } else {
                // End of folded value
                if let Some(key) = current_key.take() {
                    let value = multiline_parts.join(" ");
                    map.insert(key, value);
                }
                multiline_parts.clear();
                in_multiline = false;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, raw_val)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            if key.is_empty() {
                continue;
            }
            let val = raw_val.trim();

            // Folded scalar: `description: >`
            if val == ">" || val == "|-" || val == "|" {
                current_key = Some(key);
                in_multiline = true;
                multiline_parts.clear();
                continue;
            }

            map.insert(key, val.to_string());
        }
    }

    // Flush any remaining multiline value
    if let Some(key) = current_key {
        let value = multiline_parts.join(" ");
        map.insert(key, value);
    }

    map
}

/// Extract trigger keywords from the body text (lines under `## Triggers` or `## Triggers:`).
fn parse_triggers_from_body(body: &str) -> Vec<String> {
    let mut triggers = Vec::new();
    let mut in_triggers = false;
    let mut seen_content = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("## Triggers")
            || trimmed.eq_ignore_ascii_case("## Triggers:")
        {
            in_triggers = true;
            continue;
        }
        if in_triggers {
            if trimmed.starts_with("## ") {
                break;
            }
            if trimmed.is_empty() {
                // An empty line after seeing triggers marks the end
                if seen_content {
                    break;
                }
                continue;
            }
            let t = trimmed.trim_start_matches('-').trim();
            if !t.is_empty() {
                seen_content = true;
                triggers.push(t.to_string());
            }
        }
    }

    triggers
}

/// Load all skill metadata from a skills directory (recursively).
pub fn load_skills_from_dir(dir: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();

    if !dir.exists() {
        return skills;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension()
                && (ext == "md" || ext == "txt" || ext == "skill")
            {
                match Skill::load_from_file(&path) {
                    Ok(skill) => skills.push(skill),
                    Err(e) => {
                        tracing::warn!("Failed to load skill {}: {}", path.display(), e);
                    }
                }
            } else if path.is_dir() {
                skills.extend(load_skills_from_dir(&path));
            }
        }
    }

    skills
}

/// Load skills from default locations:
/// - `./.mp_agent/skills/` (project-local, takes precedence)
/// - `$HOME/.config/mp_agent/skills/` (global)
pub fn load_all_skills() -> Vec<Skill> {
    let mut skills = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        let skills_dir = cwd.join(".mp_agent").join("skills");
        skills.extend(load_skills_from_dir(&skills_dir));

        if let Some(home) = dirs::home_dir() {
            let home_skills = home.join(".config").join("mp_agent").join("skills");
            skills.extend(load_skills_from_dir(&home_skills));
        }
    }

    skills
}

/// Load AGENTS.md from the current directory
pub fn load_agents_md() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let agents_md = cwd.join("AGENTS.md");
    if agents_md.exists() {
        std::fs::read_to_string(agents_md).ok()
    } else {
        None
    }
}

/// Load the base system prompt from `.mp_agent/system_prompt.md`.
/// Falls back to a built-in template if absent.
pub fn load_system_prompt_template() -> String {
    let cwd = std::env::current_dir().ok().unwrap_or_default();
    let template_path = cwd.join(".mp_agent").join("system_prompt.md");

    if template_path.exists() {
        std::fs::read_to_string(&template_path).unwrap_or_else(|e| {
            tracing::warn!("Failed to read system prompt template: {}", e);
            built_in_template().to_string()
        })
    } else {
        built_in_template().to_string()
    }
}

/// Built-in fallback system prompt template.
fn built_in_template() -> &'static str {
    r#"You are mp_agent, an AI-powered coding assistant that works inside a terminal-based TUI. Your primary goal is to help the user with software engineering tasks efficiently and accurately.

## Available Tools
You have access to the following tools:

### File Operations
- **read_file** — Read a file's contents. Supports offset/limit for partial reads.
- **write_file** — Write content to a file. Creates the file if it doesn't exist. Prefer this for new files or complete rewrites.
- **edit_file** — Make targeted edits by matching an exact old_string and replacing it with new_string. **PREFER THIS** for small, focused changes to existing files. The match must be unique.
- **glob** — Find files matching a glob pattern (e.g. `**/*.rs`, `src/**/*.ts`).
- **grep** — Search file contents using regex.
- **list_directory** — List files and directories in a path.

### Execution
- **bash** — Execute shell commands. Use this for running builds, tests, git operations, and other shell tasks.

### Task Management (Todos)
- **add_todo** — Add a task to the todo list with a description and optional priority.
- **update_todo** — Mark a todo as done, update its description or priority.
- **list_todos** — Show all current todos with their status.
- **remove_todo** — Delete a todo from the list.

## Tool Usage Guidelines

### File Editing
1. Before editing a file, read it first to understand its context.
2. For small, targeted changes, ALWAYS use `edit_file` instead of `write_file`. This preserves file permissions and reduces noise.
3. Only use `write_file` when creating new files or when the edit is very large (>50% of the file changes).
4. When using `edit_file`, provide enough surrounding context in old_string to make the match unique. Include 1-2 lines before and after the change if possible.
5. After editing, verify the change by reading the relevant section of the file.

### Bash Commands
1. Use relative paths when possible.
2. For long-running commands, structure them to produce output incrementally.
3. Check exit codes — if a command fails, analyze the error and fix it.
4. Prefer running build/test commands with `--no-cache` or equivalent flags when debugging.
5. Use `&&` to chain commands when appropriate, but keep chains reasonable.

### Search & Navigation
1. Use `glob` to find files by name pattern, `grep` to find content.
2. When searching, start with a broad pattern and narrow down.
3. Use `list_directory` to explore project structure.

## Best Practices

1. **Read before write** — Always read a file before suggesting or making changes to it.
2. **Iterate** — Make small, focused changes and verify each step.
3. **Test** — After making changes, run relevant tests or build commands to verify correctness.
4. **Check syntax** — After editing a file, verify the code compiles/runs correctly.
5. **Use tools** — Don't guess about file contents or structure; use the available tools to explore.
6. **Be concise** — Provide clear, direct answers. Use markdown for formatting.
7. **Error handling** — If a tool fails, read the error message, fix the underlying issue, and retry.
8. **Permission awareness** — File modifications and bash execution require user approval. If permission is denied, explain what you were trying to do and suggest alternatives.

## Reflection & Delivery

### End-of-round reflection

At the end of each tool-use round (after receiving tool results), briefly
reflect: *"Is the current information sufficiently complete and accurate
to deliver the final answer?"* If yes, do NOT call any more tools — proceed
to deliver the answer wrapped in `<answer>` tags.

### Answer trigger

When you have the final answer ready, wrap it in `<answer>...</answer>` tags:

```
<answer>
Your final, complete response here.
</answer>
```

The system detects this tag and stops processing immediately. The tag itself
is stripped before display, so the user sees only the content inside it.

**Rules:**
- Only use `<answer>` when the task is truly resolved.
- If more information is needed, keep using tools. Do NOT guess or fabricate.
- The `<answer>` tag should contain your complete final response.

## Uncertainty & Choice

When you are uncertain about which approach to take, or when the user's request
can be addressed from multiple angles, **do NOT guess or proceed blindly**.
Instead, call the `present_choices` tool to display a dedicated choice panel
that lets the user pick their preferred direction.

### When to present choices

- The task can be solved in more than one reasonable way.
- You need clarification before proceeding.
- The user has not specified a clear preference.
- There are trade-offs between different approaches (speed vs correctness,
  quick fix vs thorough refactor, etc.).

### How to present choices

- Provide a list of 2–9 approach descriptions.
- Each choice should be concise and informative (1–2 sentences).
- The user can select by number (1–9), navigate with arrow keys, press Enter
  to confirm, press Esc to cancel, or type a custom approach of their own.
- After the user selects, continue with the chosen approach.

## MCP Tools

If MCP servers are configured, additional tools prefixed with the server name
(e.g. `git_status`, `db_query`) are available alongside the native tools listed
above. Use them when their capabilities match the task. Treat MCP tool errors
as transient — inspect the error message and retry with corrected arguments if
appropriate.

## Skills

The following skills are available (names only; use `/skills` to inspect
details or `read_file` on the file path to load full instructions).

{skills_index}

## Input & Chat Behavior

- The user can keep typing while you are processing; messages are queued and
  delivered in order once you respond. A "N queued" overlay shows pending
  messages.
- Pressing **Alt+Enter** in the input area inserts a literal newline instead
  of submitting the message (Shift+Enter requires Kitty keyboard protocol).
- Tab completion in the input area offers context suggestions extracted from
  the chat history (file paths, commands, quoted strings, and JSON argument
  strings)."#
}

/// Build the system prompt with progressive-disclosure skill loading.
///
/// Only skill **names** are injected into the prompt to keep it concise.
/// The model can read full metadata via `read_file` on the skill's path
/// or the user can type `/skills` to see all loaded skills.
pub fn build_system_prompt(skills: &[Skill], agents_md: Option<&str>) -> String {
    let template = load_system_prompt_template();

    // Build the skills index section — just names (progressive disclosure)
    let skills_index = if skills.is_empty() {
        "No skills are currently installed.".to_string()
    } else {
        let mut lines = Vec::new();
        for skill in skills {
            lines.push(format!(
                "- **{}** — file: `{}`",
                skill.name,
                skill.path.display()
            ));
        }
        lines.join("\n")
    };

    let template_with_skills = template.replace("{skills_index}", &skills_index);

    let mut parts = vec![template_with_skills];

    if let Some(agents_md_content) = agents_md {
        parts.push(format!(
            "\n## Project Context (from AGENTS.md)\n\n{}",
            agents_md_content
        ));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_skill_parse_simple() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.skill");
        fs::write(
            &path,
            "Test Skill\nThis is a test. It does nothing.\n\n## Triggers:\ntest\nnothing",
        )
        .unwrap();

        let skill = Skill::load_from_file(&path).unwrap();
        assert_eq!(skill.name, "Test Skill");
        assert_eq!(skill.description, "This is a test. It does nothing.");
        assert_eq!(skill.triggers, vec!["test", "nothing"]);
    }

    #[test]
    fn test_skill_parse_no_triggers() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("no_trigger.skill");
        fs::write(
            &path,
            "# No Trigger Skill\nJust a description, no triggers.",
        )
        .unwrap();

        let skill = Skill::load_from_file(&path).unwrap();
        assert_eq!(skill.name, "No Trigger Skill");
        assert_eq!(skill.description, "Just a description, no triggers.");
        assert!(skill.triggers.is_empty());
    }

    #[test]
    fn test_skill_parse_header_prefix() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("header.skill");
        fs::write(
            &path,
            "## Markdown Header\nDescription here.\n\n## Triggers:\nmarkdown",
        )
        .unwrap();

        let skill = Skill::load_from_file(&path).unwrap();
        assert_eq!(skill.name, "Markdown Header");
        assert_eq!(skill.description, "Description here.");
        assert_eq!(skill.triggers, vec!["markdown"]);
    }

    #[test]
    fn test_skill_parse_frontmatter() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("SKILL.md");
        fs::write(
            &path,
            "---\nname: my-test-skill\ndescription: A test skill with frontmatter.\nlicense: MIT\n---\n\n# My Test Skill\n\n## Triggers\n- trigger-a\n- trigger-b\n",
        )
        .unwrap();

        let skill = Skill::load_from_file(&path).unwrap();
        assert_eq!(skill.name, "my-test-skill");
        assert_eq!(skill.description, "A test skill with frontmatter.");
        assert_eq!(skill.triggers, vec!["trigger-a", "trigger-b"]);
    }

    #[test]
    fn test_skill_parse_frontmatter_folded_description() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("SKILL.md");
        fs::write(
            &path,
            "---\nname: folded-skill\ndescription: >\n  A multi-line\n  folded description.\n---\n\nBody.\n\n## Triggers\n- t1\n",
        )
        .unwrap();

        let skill = Skill::load_from_file(&path).unwrap();
        assert_eq!(skill.name, "folded-skill");
        assert_eq!(skill.description, "A multi-line folded description.");
        assert_eq!(skill.triggers, vec!["t1"]);
    }

    #[test]
    fn test_skill_parse_frontmatter_name_falls_back_to_stem() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("fallback-skill.skill");
        fs::write(
            &path,
            "---\ndescription: No name here.\n---\n\nBody.\n\n## Triggers\n- t1\n",
        )
        .unwrap();

        let skill = Skill::load_from_file(&path).unwrap();
        assert_eq!(skill.name, "fallback-skill");
        assert_eq!(skill.description, "No name here.");
    }

    #[test]
    fn test_parse_frontmatter_basic() {
        let (fm, body) = parse_frontmatter("---\nname: foo\ndesc: bar\n---\n\nBody text");
        assert_eq!(fm, "name: foo\ndesc: bar");
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_parse_frontmatter_no_closing() {
        let (fm, body) = parse_frontmatter("---\nname: foo");
        assert_eq!(fm, "");
        assert_eq!(body, "name: foo");
    }

    #[test]
    fn test_parse_yaml_kv_simple() {
        let map = parse_yaml_kv("name: test\ndescription: hello world");
        assert_eq!(map.get("name").map(|s| s.as_str()), Some("test"));
        assert_eq!(
            map.get("description").map(|s| s.as_str()),
            Some("hello world")
        );
    }

    #[test]
    fn test_parse_yaml_kv_folded() {
        let map = parse_yaml_kv("name: test\ndescription: >\n  multi\n  line\nkey2: val2");
        assert_eq!(map.get("name").map(|s| s.as_str()), Some("test"));
        assert_eq!(
            map.get("description").map(|s| s.as_str()),
            Some("multi line")
        );
        assert_eq!(map.get("key2").map(|s| s.as_str()), Some("val2"));
    }

    #[test]
    fn test_parse_triggers_from_body() {
        let body = "Some text\n\n## Triggers\n- foo\n- bar\n\nOther section";
        let triggers = parse_triggers_from_body(body);
        assert_eq!(triggers, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_triggers_no_triggers_section() {
        let body = "Just text\nNo triggers here.";
        let triggers = parse_triggers_from_body(body);
        assert!(triggers.is_empty());
    }

    #[test]
    fn test_load_skills_from_dir_recursive() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join(".mp_agent").join("skills");
        std::fs::create_dir_all(skills_dir.join("subdir")).unwrap();

        fs::write(
            skills_dir.join("a.skill"),
            "Skill A\nDescription A.\n\n## Triggers:\na",
        )
        .unwrap();
        fs::write(
            skills_dir.join("subdir").join("b.skill"),
            "Skill B\nDescription B.\n\n## Triggers:\nb",
        )
        .unwrap();

        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let skills = load_all_skills();
        assert_eq!(skills.len(), 2);

        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn test_build_system_prompt_includes_skill_index() {
        let skills = vec![Skill {
            name: "Test Skill".into(),
            description: "A test skill description.".into(),
            triggers: vec!["test".into()],
            path: PathBuf::from("/fake/path.skill"),
        }];

        let prompt = build_system_prompt(&skills, None);
        assert!(prompt.contains("Test Skill"));
        assert!(prompt.contains("/fake/path.skill"));
        // Description and triggers should NOT be in the prompt (progressive disclosure)
        assert!(!prompt.contains("A test skill description."));
        assert!(!prompt.contains("triggers: test"));
    }
}
