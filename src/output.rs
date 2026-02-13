//! CLI output formatting utilities

use anyhow::Result;
use colored::Colorize;
use prettytable::{Cell, Row, Table};
use serde::Serialize;

/// Renders detail views as sections with bold headers and terminal-width separators.
pub struct SectionRenderer {
    sections: Vec<(String, String)>,
}

impl SectionRenderer {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    pub fn section(mut self, header: &str, value: impl std::fmt::Display) -> Self {
        self.sections.push((header.to_string(), value.to_string()));
        self
    }

    pub fn print(self) {
        let width = console::Term::stdout().size().1 as usize;
        let separator = "─".repeat(width);

        for (header, value) in &self.sections {
            println!("{}", header.bold());
            println!("{}", separator.dimmed());
            println!("{}", value);
            println!();
        }
    }
}

/// Trait for types that can be formatted as CSV or Table output
pub trait Formattable {
    /// Column headers for CSV output
    fn csv_headers() -> &'static [&'static str];

    /// Convert this item to a CSV row
    fn to_csv_row(&self) -> Vec<String>;

    /// Column headers for table output
    fn table_headers() -> Vec<Cell>;

    /// Convert this item to a table row
    fn to_table_row(&self) -> Vec<Cell>;
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
            println!("{}", serde_json::to_string_pretty(&response)?);
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
            let mut table = Table::new();
            table.add_row(Row::new(T::table_headers()));
            for item in items {
                table.add_row(Row::new(item.to_table_row()));
            }
            table.printstd();
            println!("\nPage: {} of {}", page, total_pages);
        }
    }
    Ok(())
}
