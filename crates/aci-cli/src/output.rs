use aci_core::SourceSpan;
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) struct TableStyle {
    color: bool,
}

impl TableStyle {
    pub(crate) fn new(color: bool) -> Self {
        Self { color }
    }

    fn border(self, value: &str) -> String {
        self.paint(value, "\x1b[2m")
    }

    fn header(self, value: &str) -> String {
        self.paint(value, "\x1b[1;36m")
    }

    fn muted(self, value: &str) -> String {
        self.paint(value, "\x1b[2m")
    }

    fn paint(self, value: &str, code: &str) -> String {
        if self.color {
            format!("{code}{value}\x1b[0m")
        } else {
            value.to_string()
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

fn render_table(headers: &[&str], rows: &[Vec<String>], style: TableStyle) -> String {
    if rows.is_empty() {
        return format!("{}\n", style.muted("No results."));
    }
    let widths = column_widths(headers, rows);
    let mut output = String::new();
    push_rule(&mut output, &widths, style);
    push_row(&mut output, headers.iter().copied(), &widths, style, true);
    push_rule(&mut output, &widths, style);
    for row in rows {
        push_row(
            &mut output,
            row.iter().map(String::as_str),
            &widths,
            style,
            false,
        );
    }
    push_rule(&mut output, &widths, style);
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

fn push_rule(output: &mut String, widths: &[usize], style: TableStyle) {
    output.push_str(&style.border("+"));
    for width in widths {
        output.push_str(&style.border(&"-".repeat(width + 2)));
        output.push_str(&style.border("+"));
    }
    output.push('\n');
}

fn push_row<'a>(
    output: &mut String,
    cells: impl IntoIterator<Item = &'a str>,
    widths: &[usize],
    style: TableStyle,
    header: bool,
) {
    output.push_str(&style.border("|"));
    for (cell, width) in cells.into_iter().zip(widths) {
        output.push(' ');
        if header {
            output.push_str(&style.header(cell));
        } else {
            output.push_str(cell);
        }
        output.push_str(&" ".repeat(width - cell.len() + 1));
        output.push_str(&style.border("|"));
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
    fn renders_colorized_pretty_table_when_enabled() {
        let rows = vec![vec!["app.main".to_string()]];

        assert_eq!(
            render_table(&["Symbol"], &rows, TableStyle::new(true)),
            concat!(
                "\u{1b}[2m+\u{1b}[0m\u{1b}[2m----------\u{1b}[0m\u{1b}[2m+\u{1b}[0m\n",
                "\u{1b}[2m|\u{1b}[0m \u{1b}[1;36mSymbol\u{1b}[0m   \u{1b}[2m|\u{1b}[0m\n",
                "\u{1b}[2m+\u{1b}[0m\u{1b}[2m----------\u{1b}[0m\u{1b}[2m+\u{1b}[0m\n",
                "\u{1b}[2m|\u{1b}[0m app.main \u{1b}[2m|\u{1b}[0m\n",
                "\u{1b}[2m+\u{1b}[0m\u{1b}[2m----------\u{1b}[0m\u{1b}[2m+\u{1b}[0m\n",
            )
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
}
