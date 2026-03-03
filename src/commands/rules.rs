use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Term};
use std::path::PathBuf;

use crate::api::types::{
    rule_status_label, CreateRuleInput, RuleCreationRequestId, RuleListItem,
};
use crate::commands::bugs::resolve_repo_id;
use crate::output::{output_list, SectionRenderer};
use crate::utils::{format_date, format_datetime};

#[derive(Subcommand)]
pub enum RuleCommands {
    /// Start an async rule creation job for a repository
    Create {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo name
        repo: String,

        /// Description of the rule to create
        #[arg(long)]
        description: Option<String>,

        /// Bug ID to use as context (repeatable)
        #[arg(long = "bug-id")]
        bug_ids: Vec<String>,

        /// Commit SHA to examine for patterns (repeatable)
        #[arg(long = "commit-sha")]
        commit_shas: Vec<String>,
    },

    /// List rule creation requests for a repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo name
        repo: String,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// Show the details or status of a rule creation request
    Show {
        /// Rule creation request ID (rcr_...)
        request_id: String,
    },

    /// Persist a completed rule's files to .agents/skills/<rule_name>/
    Persist {
        /// Rule creation request ID (rcr_...)
        request_id: String,
    },
}

fn validate_create_input(
    description: &Option<String>,
    bug_ids: &[String],
    commit_shas: &[String],
) -> Result<()> {
    if description.is_none() && bug_ids.is_empty() && commit_shas.is_empty() {
        bail!("At least one of --description, --bug-id, or --commit-sha is required.");
    }
    Ok(())
}

pub async fn handle(command: &RuleCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        RuleCommands::Create {
            repo,
            description,
            bug_ids,
            commit_shas,
        } => {
            validate_create_input(description, bug_ids, commit_shas)?;

            let repo_id = resolve_repo_id(&client, repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let input = CreateRuleInput {
                description: description.clone(),
                bug_ids: bug_ids.clone(),
                commit_shas: commit_shas.clone(),
            };

            let response = client
                .create_rule(&repo_id, input)
                .await
                .context("Failed to start rule creation")?;

            Term::stdout().write_line(&format!(
                "{} Rule creation started.",
                style("✓").green(),
            ))?;
            Term::stdout().write_line(&format!(
                "  ID: {}",
                style(response.rule_creation_request_id.to_string()).bold(),
            ))?;
            Term::stdout()
                .write_line("  Use 'detail rules show <id>' to check progress.")?;
            Ok(())
        }

        RuleCommands::List { repo, format } => {
            let repo_id = resolve_repo_id(&client, repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let response = client
                .list_rules(&repo_id)
                .await
                .context("Failed to list rules")?;

            let total = response.rules.len();
            let limit = u32::try_from(total).unwrap_or(u32::MAX).max(1);
            output_list(&response.rules, total, 1, limit, format)
        }

        RuleCommands::Show { request_id } => {
            let rule_id: RuleCreationRequestId = request_id
                .as_str()
                .try_into()
                .context("Invalid rule request ID format (expected rcr_...)")?;

            let rule = client
                .get_rule(&rule_id)
                .await
                .context("Failed to fetch rule")?;

            let mut pairs: Vec<(&str, String)> = vec![
                ("ID", rule.id.to_string()),
                ("Status", rule_status_label(&rule.status).to_string()),
                ("Created", format_datetime(rule.created_at)),
            ];

            if let Some(completed_at) = rule.completed_at {
                pairs.push(("Completed", format_datetime(completed_at)));
            }
            if let Some(name) = &rule.rule_name {
                pairs.push(("Rule Name", name.clone()));
            }
            if let Some(desc) = &rule.input.description {
                pairs.push(("Description", desc.clone()));
            }
            if !rule.input.bug_ids.is_empty() {
                pairs.push(("Bug IDs", rule.input.bug_ids.join(", ")));
            }
            if !rule.input.commit_shas.is_empty() {
                pairs.push(("Commit SHAs", rule.input.commit_shas.join(", ")));
            }

            let mut renderer = SectionRenderer::new().key_value("", &pairs);

            if let Some(files) = &rule.rule_files {
                let mut sorted: Vec<(&String, &String)> = files
                    .iter()
                    // TODO: Remove this later, this is a hack
                    .filter(|(path, _)| !path.ends_with("files_to_check.json"))
                    .collect();
                sorted.sort_by_key(|(path, _)| path.as_str());
                for (path, content) in sorted {
                    renderer = renderer.markdown(path, content);
                }
            }

            renderer.print()
        }

        RuleCommands::Persist { request_id } => {
            let rule_id: RuleCreationRequestId = request_id
                .as_str()
                .try_into()
                .context("Invalid rule request ID format (expected rcr_...)")?;

            let rule = client
                .get_rule(&rule_id)
                .await
                .context("Failed to fetch rule")?;

            let files = rule
                .rule_files
                .as_ref()
                .filter(|f| !f.is_empty())
                .context("Rule has no files to persist (may still be pending)")?;

            let rule_name = rule
                .rule_name
                .as_deref()
                .unwrap_or(request_id.as_str());

            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            let out_dir: PathBuf = cwd.join(".agents").join("skills").join(rule_name);

            std::fs::create_dir_all(&out_dir).with_context(|| {
                format!("Failed to create directory {}", out_dir.display())
            })?;

            let mut written: Vec<String> = Vec::new();
            for (path, content) in files {
                if path.ends_with("files_to_check.json") {
                    continue;
                }
                let dest = out_dir.join(path);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("Failed to create directory {}", parent.display())
                    })?;
                }
                std::fs::write(&dest, content)
                    .with_context(|| format!("Failed to write {}", dest.display()))?;
                written.push(path.clone());
            }

            Term::stdout().write_line(&format!(
                "{} Persisted {} file(s) to {}",
                style("✓").green(),
                written.len(),
                style(out_dir.display()).bold(),
            ))?;
            for path in &written {
                Term::stdout().write_line(&format!("  {}", path))?;
            }
            Ok(())
        }
    }
}

impl crate::output::Formattable for RuleListItem {
    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        let header = self
            .rule_name
            .clone()
            .unwrap_or_else(|| self.id.to_string());
        let pairs = vec![
            ("ID", self.id.to_string()),
            ("Status", rule_status_label(&self.status).to_string()),
            ("Created", format_date(self.created_at)),
        ];
        (header, pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_create_rejects_all_empty() {
        let err = validate_create_input(&None, &[], &[]).unwrap_err();
        assert!(err.to_string().contains("At least one of"));
    }

    #[test]
    fn validate_create_accepts_description_only() {
        assert!(validate_create_input(&Some("no SQL".into()), &[], &[]).is_ok());
    }

    #[test]
    fn validate_create_accepts_bug_ids_only() {
        assert!(validate_create_input(&None, &["bug_1".into()], &[]).is_ok());
    }

    #[test]
    fn validate_create_accepts_commit_shas_only() {
        assert!(validate_create_input(&None, &[], &["abc1234".into()]).is_ok());
    }

    #[test]
    fn validate_create_accepts_all_fields() {
        assert!(validate_create_input(
            &Some("desc".into()),
            &["bug_1".into()],
            &["abc".into()],
        )
        .is_ok());
    }
}
