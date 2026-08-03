use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;
use homedir::my_home;

use crate::utils::vcs::repo_root;

const BUGS_SKILL_CONTENT: &str = include_str!("../../.claude/skills/detail-bugs/SKILL.md");
const RULES_SKILL_CONTENT: &str = include_str!("../../.claude/skills/detail-create-rules/SKILL.md");

#[derive(Subcommand)]
pub enum SkillCommands {
    /// Install the detail-create-rules skill
    #[command(name = "rules")]
    Rules,
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
    let base = if user { user_home()? } else { repo_root()? };
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
