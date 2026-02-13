use anyhow::{Context, Result};
use std::process::Command;

const TRIAGE_SKILL_CONTENT: &str = include_str!("../../.claude/skills/detail-bugs/SKILL.md");

fn git_root() -> Result<std::path::PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git")?;
    anyhow::ensure!(output.status.success(), "not inside a git repository");
    let root = String::from_utf8(output.stdout).context("git output was not valid UTF-8")?;
    Ok(std::path::PathBuf::from(root.trim()))
}

pub fn handle() -> Result<()> {
    let dir = git_root()?.join(".claude/skills/detail-bugs");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("SKILL.md");
    std::fs::write(&path, TRIAGE_SKILL_CONTENT)?;
    console::Term::stderr().write_line(&format!(
        "Installed detail-bugs skill to {}",
        path.display()
    ))?;
    Ok(())
}
