//! CLI output formatting utilities

use std::io::Write as _;
use std::sync::LazyLock;

use anyhow::Result;
use console::{style, Term};
use serde::Serialize;

static MARKDOWN_SKIN: LazyLock<termimad::MadSkin> = LazyLock::new(|| {
    let mut skin = termimad::MadSkin::default();
    let dim = termimad::crossterm::style::Attribute::Dim;
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

    pub fn markdown(mut self, header: &str, value: impl std::fmt::Display) -> Self {
        self.sections.push((
            header.to_string(),
            SectionContent::Markdown(value.to_string()),
        ));
        self
    }

    pub fn print(self) -> Result<()> {
        let width = self.term.size().1 as usize;
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
    /// Column headers for CSV output
    fn csv_headers() -> &'static [&'static str];

    /// Convert this item to a CSV row
    fn to_csv_row(&self) -> Vec<String>;

    /// Return a card header and key-value pairs for terminal list display
    fn to_card(&self) -> (String, Vec<(&'static str, String)>);
}

/// Generic helper to output a list of items in the requested format
pub fn output_list<T: Formattable + Serialize>(
    items: &[T],
    total: usize,
    page: u32,
    limit: u32,
    format: &crate::OutputFormat,
) -> Result<()> {
    let total_pages = (total as u32).div_ceil(limit).max(1);

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
        crate::OutputFormat::Csv => {
            use csv::Writer;
            let mut wtr = Writer::from_writer(std::io::stdout());
            wtr.write_record(T::csv_headers())?;
            for item in items {
                wtr.write_record(item.to_csv_row())?;
            }
            wtr.flush()?;
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
