use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Term};

use crate::api::types::{
    rule_status_label, CreateRuleInput, RuleCreationRequestId, RuleId, RuleListItem,
    RuleRequestStatus,
};
use crate::output::{output_list, Formattable, SectionRenderer};
use crate::utils::datetime::{format_date, format_datetime};
use crate::utils::git::resolve_repo_arg;
use crate::utils::repos::resolve_repo_id;

#[derive(Subcommand)]
pub enum RuleCommands {
    /// Submit a rule creation request for a repository
    Create {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo name.
        /// If omitted, inferred from the git remote (origin).
        repo: Option<String>,

        /// Description of the rule to create
        #[arg(long)]
        description: Option<String>,

        /// Bug IDs to use as context (comma-separated or repeat flag)
        #[arg(long, value_delimiter = ',')]
        bug_ids: Vec<String>,

        /// Commit SHAs to examine for patterns (comma-separated or repeat flag)
        #[arg(long, value_delimiter = ',')]
        commit_shas: Vec<String>,
    },

    /// Ask Detail to propose rules for a repository
    Propose {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo name.
        /// If omitted, inferred from the git remote (origin).
        repo: Option<String>,
    },

    /// Check the status of rule creation requests
    #[command(subcommand)]
    Requests(RuleRequestCommands),

    /// List completed rules for a repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo name.
        /// If omitted, inferred from the git remote (origin).
        repo: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// Show a rule's details and generated files
    Show {
        /// Rule ID (rule_...)
        rule_id: String,
    },

    /// Persist a rule's generated files locally
    Persist {
        /// Rule ID (rule_...)
        rule_id: String,

        /// Skill directory to write detail-rules/ into (defaults to .claude/skills/)
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum RuleRequestCommands {
    /// List rule creation requests for a repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo name.
        /// If omitted, inferred from the git remote (origin).
        repo: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// Show details and status of a rule creation request
    Show {
        /// Rule creation request ID (rcr_...)
        request_id: String,
    },
}

fn validate_create_input(
    description: Option<&str>,
    bug_ids: &[String],
    commit_shas: &[String],
) -> Result<()> {
    if description.is_none() && bug_ids.is_empty() && commit_shas.is_empty() {
        bail!("At least one of --description, --bug-ids, or --commit-shas is required.");
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
            validate_create_input(description.as_deref(), bug_ids, commit_shas)?;

            let repo = resolve_repo_arg(repo.as_deref())?;
            let repo_id = resolve_repo_id(&client, &repo)
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

            Term::stdout()
                .write_line(&format!("{} Rule creation started.", style("✓").green(),))?;
            Term::stdout().write_line(&format!(
                "  Request ID: {}",
                style(response.rule_creation_request_id.to_string()).bold(),
            ))?;
            Term::stdout()
                .write_line("  Use 'detail rules requests show <id>' to check progress.")?;
            Ok(())
        }

        RuleCommands::List { repo, format } => {
            let repo = resolve_repo_arg(repo.as_deref())?;
            let repo_id = resolve_repo_id(&client, &repo)
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

        RuleCommands::Show { rule_id } => {
            let rule_id: RuleId = rule_id
                .as_str()
                .try_into()
                .context("Invalid rule ID format (expected rule_...)")?;

            let rule = client
                .get_rule(&rule_id)
                .await
                .context("Failed to fetch rule")?;

            let pairs: Vec<(&str, String)> = vec![
                ("ID", rule.id.to_string()),
                ("Name", rule.rule_name.clone()),
                ("Created", format_datetime(rule.created_at)),
            ];

            let mut renderer = SectionRenderer::new().key_value("", &pairs);

            let mut sorted: Vec<(&String, &String)> = rule.rule_files.iter().collect();
            sorted.sort_by_key(|(path, _)| path.as_str());
            for (path, content) in sorted {
                renderer = renderer.markdown(path, content);
            }

            renderer.print()
        }

        RuleCommands::Persist { rule_id, output } => {
            let rule_id: RuleId = rule_id
                .as_str()
                .try_into()
                .context("Invalid rule ID format (expected rule_...)")?;

            let rule = client
                .get_rule(&rule_id)
                .await
                .context("Failed to fetch rule")?;

            let cwd = env::current_dir().context("Failed to get current directory")?;
            let parent: PathBuf = output.as_ref().map_or_else(
                || cwd.join(".claude").join("skills"),
                |p| {
                    if p.is_absolute() {
                        p.clone()
                    } else {
                        cwd.join(p)
                    }
                },
            );
            let out_dir = parent.join("detail-rules");

            fs::create_dir_all(&out_dir)
                .with_context(|| format!("Failed to create directory {}", out_dir.display()))?;

            let mut sorted: Vec<(&String, &String)> = rule.rule_files.iter().collect();
            sorted.sort_by_key(|(path, _)| path.as_str());

            let mut written: Vec<String> = Vec::new();
            for (path, content) in sorted {
                // Reject paths with absolute or traversal components (e.g. /etc/.. or ../../foo).
                let is_safe = Path::new(path)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)));
                if !is_safe {
                    bail!("API returned an unsafe file path: {path}");
                }

                let dest = out_dir.join(path);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("Failed to create directory {}", parent.display())
                    })?;
                }
                fs::write(&dest, content)
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
                Term::stdout().write_line(&format!("  {path}"))?;
            }
            Ok(())
        }

        RuleCommands::Propose { repo } => {
            let repo = resolve_repo_arg(repo.as_deref())?;
            let repo_id = resolve_repo_id(&client, &repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let response = client
                .create_rule(&repo_id, CreateRuleInput::default())
                .await
                .context("Failed to start rule proposal")?;

            Term::stdout().write_line(&format!("{} Rule proposal started.", style("✓").green()))?;
            Term::stdout().write_line(&format!(
                "  Request ID: {}",
                style(response.rule_creation_request_id.to_string()).bold(),
            ))?;
            Term::stdout()
                .write_line("  Use 'detail rules requests show <id>' to check progress.")?;
            Ok(())
        }

        RuleCommands::Requests(sub) => match sub {
            RuleRequestCommands::List { repo, format } => {
                let repo = resolve_repo_arg(repo.as_deref())?;
                let repo_id = resolve_repo_id(&client, &repo)
                    .await
                    .context("Failed to resolve repository identifier")?;

                let response = client
                    .list_rule_requests(&repo_id)
                    .await
                    .context("Failed to list rule requests")?;

                let total = response.requests.len();
                let limit = u32::try_from(total).unwrap_or(u32::MAX).max(1);
                output_list(&response.requests, total, 1, limit, format)
            }

            RuleRequestCommands::Show { request_id } => {
                let rcr_id: RuleCreationRequestId = request_id
                    .as_str()
                    .try_into()
                    .context("Invalid request ID format (expected rcr_...)")?;

                let req = client
                    .get_rule_request(&rcr_id)
                    .await
                    .context("Failed to fetch rule creation request")?;

                let mut pairs: Vec<(&str, String)> = vec![
                    ("ID", req.id.to_string()),
                    ("Status", rule_status_label(&req.status).to_string()),
                    ("Created", format_datetime(req.created_at)),
                ];

                if let Some(completed_at) = req.completed_at {
                    pairs.push(("Completed", format_datetime(completed_at)));
                }
                if let Some(desc) = &req.input.description {
                    pairs.push(("Description", desc.clone()));
                }
                if !req.input.bug_ids.is_empty() {
                    pairs.push(("Bug IDs", req.input.bug_ids.join(", ")));
                }
                if !req.input.commit_shas.is_empty() {
                    pairs.push(("Commit SHAs", req.input.commit_shas.join(", ")));
                }

                let mut renderer = SectionRenderer::new().key_value("", &pairs);

                if !req.results.is_empty() {
                    let results: Vec<(&str, String)> = req
                        .results
                        .iter()
                        .map(|r| ("", format!("{} — {}", *r.id, r.rule_name)))
                        .collect();
                    renderer = renderer.key_value("Rules Created", &results);
                }

                renderer.print()
            }
        },
    }
}

impl Formattable for RuleRequestStatus {
    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        let mut pairs = vec![
            ("Status", rule_status_label(&self.status).to_string()),
            ("Created", format_date(self.created_at)),
        ];

        if let Some(desc) = &self.input.description {
            pairs.push(("Description", desc.clone()));
        }
        if !self.input.bug_ids.is_empty() {
            pairs.push(("Bug IDs", self.input.bug_ids.join(", ")));
        }
        if !self.input.commit_shas.is_empty() {
            pairs.push(("Commit SHAs", self.input.commit_shas.join(", ")));
        }
        pairs.push(("Rules", self.results.len().to_string()));

        (self.id.to_string(), pairs)
    }
}

impl Formattable for RuleListItem {
    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        let pairs = vec![
            ("ID", self.id.to_string()),
            ("Created", format_date(self.created_at)),
        ];
        (self.rule_name.clone(), pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_create_rejects_all_empty() {
        let err = validate_create_input(None, &[], &[]).unwrap_err();
        assert!(err.to_string().contains("At least one of"));
    }

    #[test]
    fn validate_create_accepts_description_only() {
        assert!(validate_create_input(Some("no SQL"), &[], &[]).is_ok());
    }

    #[test]
    fn validate_create_accepts_bug_ids_only() {
        assert!(validate_create_input(None, &["bug_1".into()], &[]).is_ok());
    }

    #[test]
    fn validate_create_accepts_commit_shas_only() {
        assert!(validate_create_input(None, &[], &["abc1234".into()]).is_ok());
    }

    #[test]
    fn validate_create_accepts_all_fields() {
        assert!(validate_create_input(Some("desc"), &["bug_1".into()], &["abc".into()],).is_ok());
    }
}
