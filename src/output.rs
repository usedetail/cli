//! CLI output formatting utilities

use anyhow::Result;
use prettytable::{Cell, Row, Table};
use serde::Serialize;

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
    format: &crate::OutputFormat,
) -> Result<()> {
    match format {
        crate::OutputFormat::Json => {
            let response = serde_json::json!({
                "items": items,
                "total": total,
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
            println!("\nTotal: {}", total);
        }
    }
    Ok(())
}
