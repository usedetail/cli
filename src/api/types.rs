use prettytable::Cell;
use serde::{Deserialize, Deserializer, Serialize};

// Helper to deserialize timestamps that can be either string or number
fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(i64),
    }

    match StringOrInt::deserialize(deserializer)? {
        StringOrInt::Int(i) => Ok(i),
        StringOrInt::String(s) => s.parse::<i64>().map_err(Error::custom),
    }
}

// Macro to generate type-safe ID newtypes with validation
macro_rules! define_id_type {
    ($name:ident, $prefix:literal, $type_name:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Result<Self, String> {
                let id_str = id.into();
                if !id_str.starts_with($prefix) {
                    return Err(format!(
                        "Invalid {} ID: must start with '{}', got '{}'",
                        $type_name, $prefix, id_str
                    ));
                }
                Ok(Self(id_str))
            }

            #[allow(dead_code)]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                $name::new(s).map_err(serde::de::Error::custom)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

// Define all ID types with their prefixes
define_id_type!(BugId, "bug_", "bug");
define_id_type!(RepoId, "repo_", "repository");
define_id_type!(OrgId, "org_", "organization");
define_id_type!(BugReviewId, "bfrv_", "bug review");

#[derive(Debug, Deserialize, Serialize)]
pub struct UserInfo {
    pub email: String,
    pub orgs: Vec<OrgInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrgInfo {
    pub id: OrgId,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BugsResponse {
    pub bugs: Vec<Bug>,
    pub total: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Bug {
    pub id: BugId,
    pub title: String,
    pub summary: String,
    #[serde(rename = "filePath")]
    pub file_path: Option<String>,
    #[serde(rename = "createdAt", deserialize_with = "deserialize_timestamp")]
    pub created_at: i64,
    pub review: Option<BugReview>,
    #[serde(rename = "repoId")]
    pub repo_id: RepoId,
    #[serde(rename = "commitSha")]
    pub commit_sha: Option<String>,
    #[serde(rename = "isSecurityVulnerability")]
    pub is_security_vulnerability: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BugReview {
    pub id: BugReviewId,
    pub state: BugReviewState,
    #[serde(rename = "createdAt", deserialize_with = "deserialize_timestamp")]
    pub created_at: i64,
    #[serde(rename = "dismissalReason")]
    pub dismissal_reason: Option<BugDismissalReason>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BugReviewState {
    Pending,
    Resolved,
    Dismissed,
}

impl std::fmt::Display for BugReviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Resolved => write!(f, "Resolved"),
            Self::Dismissed => write!(f, "Dismissed"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BugDismissalReason {
    NotABug,
    WontFix,
    Duplicate,
    Other,
}

impl std::fmt::Display for BugDismissalReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotABug => write!(f, "Not a Bug"),
            Self::WontFix => write!(f, "Won't Fix"),
            Self::Duplicate => write!(f, "Duplicate"),
            Self::Other => write!(f, "Other"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BugReviewRequest {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismissal_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReposResponse {
    pub repos: Vec<Repo>,
    pub total: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    #[serde(rename = "ownerName")]
    pub owner_name: String,
    #[serde(rename = "fullName")]
    pub full_name: String,
    pub visibility: String,
    #[serde(rename = "primaryBranch")]
    pub primary_branch: String,
    #[serde(rename = "orgId")]
    pub org_id: OrgId,
    #[serde(rename = "orgName")]
    pub org_name: String,
}

// Implement Formattable for Bug
impl crate::output::Formattable for Bug {
    fn csv_headers() -> &'static [&'static str] {
        &["id", "title", "file", "created"]
    }

    fn to_csv_row(&self) -> Vec<String> {
        let created_date = crate::utils::format_date(self.created_at);

        vec![
            self.id.to_string(),
            self.title.clone(),
            self.file_path.as_deref().unwrap_or("-").to_string(),
            created_date,
        ]
    }

    fn table_headers() -> Vec<Cell> {
        vec![
            Cell::new("ID"),
            Cell::new("Title"),
            Cell::new("File"),
            Cell::new("Created"),
        ]
    }

    fn to_table_row(&self) -> Vec<Cell> {
        let created_date = crate::utils::format_date(self.created_at);

        // Wrap long fields for better table display
        let wrapped_title = crate::utils::wrap_text(&self.title, 50);
        let wrapped_file = crate::utils::wrap_path(
            self.file_path.as_deref().unwrap_or("-"),
            40
        );

        vec![
            Cell::new(self.id.as_str()),
            Cell::new(&wrapped_title),
            Cell::new(&wrapped_file),
            Cell::new(&created_date),
        ]
    }
}

// Implement Formattable for Repo
impl crate::output::Formattable for Repo {
    fn csv_headers() -> &'static [&'static str] {
        &["repository", "organization"]
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.full_name.clone(),
            self.org_name.clone(),
        ]
    }

    fn table_headers() -> Vec<Cell> {
        vec![
            Cell::new("Repository"),
            Cell::new("Organization"),
        ]
    }

    fn to_table_row(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.full_name),
            Cell::new(&self.org_name),
        ]
    }
}
