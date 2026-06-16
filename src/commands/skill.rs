use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;

use anyhow::{Context, Result};
use clap::Subcommand;
use homedir::my_home;

const BUGS_SKILL_CONTENT: &str = include_str!("../../.claude/skills/detail-bugs/SKILL.md");
const RULES_SKILL_CONTENT: &str = include_str!("../../.claude/skills/detail-create-rules/SKILL.md");

#[derive(Subcommand)]
pub enum SkillCommands {
    /// Install the detail-create-rules skill
    #[command(name = "rules")]
    Rules,
}

fn parse_git_root_output(success: bool, stdout: &[u8]) -> Result<PathBuf> {
    anyhow::ensure!(success, "not inside a git repository");
    let root = str::from_utf8(stdout).context("git output was not valid UTF-8")?;
    Ok(PathBuf::from(root.trim()))
}

fn skill_install_path(base: &Path, skill_name: &str) -> PathBuf {
    base.join(".claude")
        .join("skills")
        .join(skill_name)
        .join("SKILL.md")
}

fn user_home() -> Result<PathBuf> {
    my_home()
        .context("failed to determine home directory")?
        .context("home directory not found")
}

fn git_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git")?;
    parse_git_root_output(output.status.success(), &output.stdout)
}

fn install_skill(repo_root: &Path, skill_name: &str, content: &str) -> Result<()> {
    let path = skill_install_path(repo_root, skill_name);
    let dir = path
        .parent()
        .context("failed to compute skill install directory")?;
    fs::create_dir_all(dir)?;
    fs::write(&path, content)?;
    console::Term::stderr().write_line(&format!(
        "Installed {skill_name} skill to {}",
        path.display()
    ))?;
    Ok(())
}

pub fn handle(command: Option<&SkillCommands>, user: bool) -> Result<()> {
    let base = if user { user_home()? } else { git_root()? };
    match command {
        None => install_skill(&base, "detail-bugs", BUGS_SKILL_CONTENT),
        Some(SkillCommands::Rules) => {
            install_skill(&base, "detail-create-rules", RULES_SKILL_CONTENT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_git_root_trims_newline() {
        let root = parse_git_root_output(true, b"/tmp/repo\n").unwrap();
        assert_eq!(root, PathBuf::from("/tmp/repo"));
    }

    #[test]
    fn parse_git_root_errors_when_not_in_repo() {
        let err = parse_git_root_output(false, b"").unwrap_err();
        assert!(err.to_string().contains("not inside a git repository"));
    }

    #[test]
    fn parse_git_root_errors_on_invalid_utf8() {
        let err = parse_git_root_output(true, &[0xff]).unwrap_err();
        assert!(err.to_string().contains("git output was not valid UTF-8"));
    }

    #[test]
    fn skill_install_path_is_repo_relative() {
        let path = skill_install_path(Path::new("/work/repo"), "detail-bugs");
        assert_eq!(
            path,
            PathBuf::from("/work/repo/.claude/skills/detail-bugs/SKILL.md")
        );
    }

    #[test]
    fn rules_install_path_is_repo_relative() {
        let path = skill_install_path(Path::new("/work/repo"), "detail-create-rules");
        assert_eq!(
            path,
            PathBuf::from("/work/repo/.claude/skills/detail-create-rules/SKILL.md")
        );
    }

    #[test]
    fn skill_install_path_user_level() {
        let path = skill_install_path(Path::new("/home/alice"), "detail-bugs");
        assert_eq!(
            path,
            PathBuf::from("/home/alice/.claude/skills/detail-bugs/SKILL.md")
        );
    }
}
