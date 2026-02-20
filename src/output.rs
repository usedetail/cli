//! CLI output formatting utilities

use std::fmt::Display;
use std::io::Write as _;
use std::sync::LazyLock;

use anyhow::Result;
use console::{style, Term};
use serde::Serialize;
use termimad::crossterm::style::Attribute;

static MARKDOWN_SKIN: LazyLock<termimad::MadSkin> = LazyLock::new(|| {
    let mut skin = termimad::MadSkin::default();
    let dim = Attribute::Dim;
    skin.code_block.compound_style = termimad::CompoundStyle::with_attr(dim);
    skin.inline_code = termimad::CompoundStyle::with_attr(dim);
    for h in &mut skin.headers {
        h.align = termimad::Alignment::Left;
    }
    skin
});

enum SectionContent {
    KeyValue(Vec<(String, String)>, usize),
    Markdown(String),
}

/// Renders detail views as sections with bold headers and terminal-width separators.
pub struct SectionRenderer {
    term: Term,
    sections: Vec<(String, SectionContent)>,
}

impl Default for SectionRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl SectionRenderer {
    pub fn new() -> Self {
        Self {
            term: Term::stdout(),
            sections: Vec::new(),
        }
    }

    /// Add a section with key-value pairs rendered as aligned rows with bold keys.
    pub fn key_value(mut self, header: &str, pairs: &[(&str, String)]) -> Self {
        let max_key = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        self.sections
            .push((header.to_string(), SectionContent::KeyValue(owned, max_key)));
        self
    }

    pub fn markdown(mut self, header: &str, value: impl Display) -> Self {
        self.sections.push((
            header.to_string(),
            SectionContent::Markdown(value.to_string()),
        ));
        self
    }

    pub fn print(self) -> Result<()> {
        let width = self.term.size().1.into();
        let separator = "─".repeat(width);

        for (i, (header, content)) in self.sections.iter().enumerate() {
            if !header.is_empty() {
                self.term.write_line(&format!("{}", style(header).bold()))?;
            }
            // Show separator between sections or under non-empty headers
            if i > 0 || !header.is_empty() {
                self.term
                    .write_line(&format!("{}", style(&separator).dim()))?;
            }
            match content {
                SectionContent::KeyValue(pairs, max_key) => {
                    for (k, v) in pairs {
                        self.term.write_line(&format!(
                            "{:<width$}  {}",
                            style(k).bold(),
                            v,
                            width = max_key
                        ))?;
                    }
                }
                SectionContent::Markdown(text) => {
                    write!(&self.term, "{}", MARKDOWN_SKIN.term_text(text))?;
                }
            }
            self.term.write_line("")?;
        }
        Ok(())
    }
}

/// Trait for types that can be formatted for list output
pub trait Formattable {
    /// Return a card header and key-value pairs for terminal list display
    fn to_card(&self) -> (String, Vec<(&'static str, String)>);
}

/// Compute the total number of pages for a given item count and page size.
fn total_pages(total: usize, limit: u32) -> u32 {
    u32::try_from(total)
        .unwrap_or(u32::MAX)
        .div_ceil(limit)
        .max(1)
}

/// Generic helper to output a list of items in the requested format
pub fn output_list<T: Formattable + Serialize>(
    items: &[T],
    total: usize,
    page: u32,
    limit: u32,
    format: &crate::OutputFormat,
) -> Result<()> {
    let total_pages = total_pages(total, limit);

    match format {
        crate::OutputFormat::Json => {
            let response = serde_json::json!({
                "items": items,
                "total": total,
                "page": page,
                "total_pages": total_pages,
            });
            Term::stdout().write_line(&serde_json::to_string_pretty(&response)?)?;
        }
        crate::OutputFormat::Table => {
            let term = Term::stdout();
            let max_key = items
                .iter()
                .flat_map(|item| item.to_card().1)
                .map(|(k, _)| k.len())
                .max()
                .unwrap_or(0);
            for (i, item) in items.iter().enumerate() {
                let (header, pairs) = item.to_card();
                term.write_line(&format!("{}. {}", i + 1, header))?;
                for (k, v) in &pairs {
                    term.write_line(&format!("    {:<width$}  {}", k, v, width = max_key))?;
                }
            }
            term.write_line(&format!("\nPage: {} of {}", page, total_pages))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── total_pages ──────────────────────────────────────────────────

    #[test]
    fn total_pages_exact_division() {
        assert_eq!(total_pages(100, 50), 2);
    }

    #[test]
    fn total_pages_with_remainder() {
        assert_eq!(total_pages(101, 50), 3);
    }

    #[test]
    fn total_pages_single_page() {
        assert_eq!(total_pages(10, 50), 1);
    }

    #[test]
    fn total_pages_empty_list_returns_one() {
        assert_eq!(total_pages(0, 50), 1);
    }

    #[test]
    fn total_pages_limit_one() {
        assert_eq!(total_pages(5, 1), 5);
    }

    // ── SectionRenderer builder ──────────────────────────────────────

    #[test]
    fn section_renderer_default_creates_empty() {
        let renderer = SectionRenderer::default();
        assert!(renderer.sections.is_empty());
    }

    #[test]
    fn section_renderer_key_value_adds_section() {
        let renderer = SectionRenderer::new().key_value("Info", &[("key", "val".to_string())]);
        assert_eq!(renderer.sections.len(), 1);
        assert_eq!(renderer.sections[0].0, "Info");
    }

    #[test]
    fn section_renderer_markdown_adds_section() {
        let renderer = SectionRenderer::new().markdown("Body", "hello");
        assert_eq!(renderer.sections.len(), 1);
        assert_eq!(renderer.sections[0].0, "Body");
    }

    #[test]
    fn section_renderer_chaining() {
        let renderer = SectionRenderer::new()
            .key_value("A", &[])
            .markdown("B", "text")
            .key_value("C", &[("x", "y".to_string())]);
        assert_eq!(renderer.sections.len(), 3);
    }
}
