//! Checks that every skill under `.claude/skills` has frontmatter that
//! strict YAML parsers accept. Claude Code tolerates some invalid YAML, but
//! other installers (such as the Skills CLI) skip a skill whose frontmatter
//! does not parse.

#![allow(
    clippy::expect_used,
    clippy::tests_outside_test_module,
    reason = "expect is idiomatic in tests"
)]

use std::fs;
use std::path::{Path, PathBuf};

use yaml_rust2::YamlLoader;

fn skill_dirs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".claude/skills");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read .claude/skills")
        .map(|entry| entry.expect("read skill dir entry").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Returns the lines between an opening `---` on the first line and the next
/// `---` line. `str::lines` accepts both LF and CRLF line endings.
fn frontmatter(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut block = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(block.join("\n"));
        }
        block.push(line);
    }
    None
}

fn check_skill(dir: &Path) -> Result<(), String> {
    let path = dir.join("SKILL.md");
    if !path.is_file() {
        return Err("missing SKILL.md".to_owned());
    }
    let content = fs::read_to_string(&path).map_err(|err| format!("could not read file: {err}"))?;
    let raw = frontmatter(&content).ok_or(
        "no frontmatter block found: SKILL.md must start with a `---` line and the \
         frontmatter must end with another `---` line",
    )?;
    let docs = YamlLoader::load_from_str(&raw)
        .map_err(|err| format!("frontmatter is not valid YAML: {err}"))?;
    let doc = docs.first().ok_or("frontmatter is empty")?;

    let dir_name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("could not determine skill directory name")?;
    if doc["name"].as_str() != Some(dir_name) {
        return Err(format!(
            "`name` must be a string matching the skill directory `{dir_name}`"
        ));
    }
    if doc["description"]
        .as_str()
        .is_none_or(|desc| desc.trim().is_empty())
    {
        return Err("`description` must be a non-empty string".to_owned());
    }
    Ok(())
}

#[test]
fn skill_frontmatter_is_valid_yaml() {
    let dirs = skill_dirs();
    assert!(!dirs.is_empty(), "expected at least one skill directory");

    let failures: Vec<String> = dirs
        .iter()
        .filter_map(|dir| {
            check_skill(dir)
                .err()
                .map(|err| format!("{}: {err}", dir.display()))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "invalid skills:\n{}",
        failures.join("\n")
    );
}

#[test]
fn frontmatter_accepts_crlf_and_closing_delimiter_at_eof() {
    let expected = Some("name: a\ndescription: b".to_owned());
    assert_eq!(
        frontmatter("---\r\nname: a\r\ndescription: b\r\n---\r\nbody\r\n"),
        expected,
        "CRLF line endings"
    );
    assert_eq!(
        frontmatter("---\nname: a\ndescription: b\n---"),
        expected,
        "closing delimiter at end of file"
    );
    assert_eq!(frontmatter("name: a\n"), None, "no opening delimiter");
    assert_eq!(frontmatter("---\nname: a\n"), None, "no closing delimiter");
}
