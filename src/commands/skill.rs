use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

const SKILL_CONTENT: &str = include_str!("../../.claude/skills/detail-bugs/SKILL.md");

fn git_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git")?;
    anyhow::ensure!(output.status.success(), "not inside a git repository");
    let root = String::from_utf8(output.stdout).context("git output was not valid UTF-8")?;
    Ok(PathBuf::from(root.trim()))
}

pub fn handle() -> Result<()> {
    let dir = git_root()?.join(".claude/skills/detail-bugs");
    fs::create_dir_all(&dir)?;
    let path = dir.join("SKILL.md");
    fs::write(&path, SKILL_CONTENT)?;
    console::Term::stderr().write_line(&format!(
        "Installed detail-bugs skill to {}",
        path.display()
    ))?;
    Ok(())
}
