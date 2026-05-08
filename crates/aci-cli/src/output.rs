use aci_core::SourceSpan;
use std::path::Path;
use std::time::Duration;

/// ANSI escape codes for terminal styling.
#[allow(dead_code)]
mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
}

#[derive(Clone, Copy)]
pub(crate) struct TableStyle {
    color: bool,
}

impl TableStyle {
    pub(crate) fn new(color: bool) -> Self {
        Self { color }
    }

    pub(crate) fn color_enabled(self) -> bool {
        self.color
    }

    fn header(self, value: &str) -> String {
        self.paint(value, &format!("{}{}", ansi::BOLD, ansi::CYAN))
    }

    fn muted(self, value: &str) -> String {
        self.paint(value, ansi::DIM)
    }

    fn paint(self, value: &str, code: &str) -> String {
        if self.color {
            format!("{code}{value}{}", ansi::RESET)
        } else {
            value.to_string()
        }
    }
}

/// Shared output utilities for colored CLI output.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct Output {
    color: bool,
}

#[allow(dead_code)]
impl Output {
    pub(crate) fn new(color: bool) -> Self {
        Self { color }
    }

    /// Format a label in bold cyan (e.g., "indexed", "watching").
    pub(crate) fn label(&self, text: &str) -> String {
        if self.color {
            format!("{}{}{}{}", ansi::BOLD, ansi::CYAN, text, ansi::RESET)
        } else {
            text.to_string()
        }
    }

    /// Format a number/value in bold.
    pub(crate) fn value<T: std::fmt::Display>(&self, val: T) -> String {
        if self.color {
            format!("{}{}{}", ansi::BOLD, val, ansi::RESET)
        } else {
            val.to_string()
        }
    }

    /// Format text in green (for success states).
    pub(crate) fn success(&self, text: &str) -> String {
        if self.color {
            format!("{}{}{}", ansi::GREEN, text, ansi::RESET)
        } else {
            text.to_string()
        }
    }

    /// Format text in yellow (for warnings or skipped items).
    pub(crate) fn warning(&self, text: &str) -> String {
        if self.color {
            format!("{}{}{}", ansi::YELLOW, text, ansi::RESET)
        } else {
            text.to_string()
        }
    }

    /// Format a path in blue.
    pub(crate) fn path(&self, text: &str) -> String {
        if self.color {
            format!("{}{}{}", ansi::BLUE, text, ansi::RESET)
        } else {
            text.to_string()
        }
    }

    /// Format dimmed/secondary text.
    pub(crate) fn dim(&self, text: &str) -> String {
        if self.color {
            format!("{}{}{}", ansi::DIM, text, ansi::RESET)
        } else {
            text.to_string()
        }
    }
}

pub(crate) fn print_table<I, R, C>(headers: &[&str], rows: I, style: TableStyle)
where
    I: IntoIterator<Item = R>,
    R: IntoIterator<Item = C>,
    C: ToString,
{
    let rows = rows
        .into_iter()
        .map(|row| row.into_iter().map(|cell| cell.to_string()).collect())
        .collect::<Vec<Vec<String>>>();
    print!("{}", render_table(headers, &rows, style));
}

pub(crate) fn format_location(path: Option<&Path>, span: Option<&SourceSpan>) -> Option<String> {
    let path = path.map(|path| path.display().to_string());
    match (path, span) {
        (Some(path), Some(span)) => Some(format!(
            "{}:{}:{}",
            path, span.start.line, span.start.column
        )),
        (Some(path), None) => Some(path),
        (None, Some(span)) => Some(format!("{}:{}", span.start.line, span.start.column)),
        (None, None) => None,
    }
}

pub(crate) fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds >= 1.0 {
        format!("{seconds:.2}s")
    } else {
        format!("{:.0}ms", seconds * 1000.0)
    }
}

fn render_table(headers: &[&str], rows: &[Vec<String>], style: TableStyle) -> String {
    if rows.is_empty() {
        return format!("{}\n", style.muted("No results."));
    }
    let widths = column_widths(headers, rows);
    let mut output = String::new();

    // Simplified table format when colors are enabled
    if style.color_enabled() {
        // Header row with bold cyan styling
        for (i, (header, width)) in headers.iter().zip(&widths).enumerate() {
            if i > 0 {
                output.push_str("  ");
            }
            output.push_str(&style.header(&format!("{:width$}", header, width = *width)));
        }
        output.push('\n');

        // Data rows
        for row in rows {
            for (i, (cell, width)) in row.iter().zip(&widths).enumerate() {
                if i > 0 {
                    output.push_str("  ");
                }
                output.push_str(&format!("{:width$}", cell, width = *width));
            }
            output.push('\n');
        }
    } else {
        // ASCII table format when colors are disabled
        push_rule(&mut output, &widths);
        push_row(
            &mut output,
            headers.iter().copied(),
            &widths,
            Some(&style),
            true,
        );
        push_rule(&mut output, &widths);
        for row in rows {
            push_row(
                &mut output,
                row.iter().map(String::as_str),
                &widths,
                None,
                false,
            );
        }
        push_rule(&mut output, &widths);
    }
    output
}

fn column_widths(headers: &[&str], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = headers
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();
    for row in rows {
        if row.len() > widths.len() {
            widths.resize(row.len(), 0);
        }
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }
    widths
}

fn push_rule(output: &mut String, widths: &[usize]) {
    output.push('+');
    for width in widths {
        output.push_str(&"-".repeat(width + 2));
        output.push('+');
    }
    output.push('\n');
}

fn push_row<'a>(
    output: &mut String,
    cells: impl IntoIterator<Item = &'a str>,
    widths: &[usize],
    style: Option<&TableStyle>,
    header: bool,
) {
    output.push('|');
    for (cell, width) in cells.into_iter().zip(widths) {
        output.push(' ');
        if header {
            if let Some(s) = style {
                output.push_str(&s.header(cell));
            } else {
                output.push_str(cell);
            }
        } else {
            output.push_str(cell);
        }
        output.push_str(&" ".repeat(width - cell.len() + 1));
        output.push('|');
    }
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_pretty_table_with_aligned_columns() {
        let rows = vec![
            vec!["app.main".to_string(), "Function".to_string()],
            vec!["Service.run".to_string(), "Method".to_string()],
        ];

        assert_eq!(
            render_table(&["Symbol", "Kind"], &rows, TableStyle::new(false)),
            concat!(
                "+-------------+----------+\n",
                "| Symbol      | Kind     |\n",
                "+-------------+----------+\n",
                "| app.main    | Function |\n",
                "| Service.run | Method   |\n",
                "+-------------+----------+\n",
            )
        );
    }

    #[test]
    fn renders_empty_pretty_table_as_message() {
        assert_eq!(
            render_table(&["Symbol"], &[], TableStyle::new(false)),
            "No results.\n"
        );
    }

    #[test]
    fn renders_simplified_colorized_table_when_enabled() {
        let rows = vec![vec!["app.main".to_string()]];

        // With colors enabled, renders simplified format without borders
        assert_eq!(
            render_table(&["Symbol"], &rows, TableStyle::new(true)),
            concat!("\u{1b}[1m\u{1b}[36mSymbol  \u{1b}[0m\n", "app.main\n",)
        );
    }

    #[test]
    fn formats_path_and_start_position_as_location() {
        let span = SourceSpan::new(
            10,
            20,
            aci_core::LineColumn::new(4, 7),
            aci_core::LineColumn::new(4, 17),
        );

        assert_eq!(
            format_location(Some(Path::new("src/main.rs")), Some(&span)),
            Some("src/main.rs:4:7".to_string())
        );
    }

    #[test]
    fn output_formats_labels_and_values() {
        let output = Output::new(true);
        assert!(output.label("indexed").contains("\x1b[1m"));
        assert!(output.label("indexed").contains("\x1b[36m"));
        assert!(output.value(42).contains("\x1b[1m"));
        assert!(output.success("done").contains("\x1b[32m"));
        assert!(output.warning("skipped").contains("\x1b[33m"));
        assert!(output.path("/foo").contains("\x1b[34m"));
        assert!(output.dim("secondary").contains("\x1b[2m"));

        let plain = Output::new(false);
        assert_eq!(plain.label("indexed"), "indexed");
        assert_eq!(plain.value(42), "42");
    }
}
