//! A small, strict CSV reader for imports (D69, D70): the `obop import` and `obop admin import`
//! commands and the `csv` input of `bulk_create`. RFC 4180 quoting (`""` inside quotes, line
//! breaks inside quotes), a leading byte order mark dropped, the delimiter chosen between `,` and
//! `;` only (from the header line), and hard limits on size, rows, columns and cell length so a
//! hostile file cannot make the server allocate without bound.
//!
//! This is an import reader. A cell that begins with `=`, `+`, `-` or `@` is kept exactly as
//! written: the spreadsheet "formula injection" risk is about *writing* untrusted text into a
//! file someone opens in a spreadsheet, so an exporter has to neutralise such cells; the reader
//! never evaluates anything. Nothing here is lenient about structure: a stray quote, an
//! unterminated quote or a short row is an error that names the line.

/// Largest input accepted, in bytes.
pub const MAX_BYTES: usize = 1_048_576;
/// Most data rows (the header is not counted).
pub const MAX_ROWS: usize = 1000;
/// Most columns.
pub const MAX_COLUMNS: usize = 100;
/// Longest cell, in bytes.
pub const MAX_CELL_BYTES: usize = 65_536;

/// A parsed file: the header names and the data rows, each as long as the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The delimiter that was detected (`,` or `;`).
    pub delimiter: char,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// The line of the file on which each data row starts (1 based), for error messages.
    pub lines: Vec<usize>,
}

/// Parse `bytes` with at most `max_rows` data rows (never more than [`MAX_ROWS`]).
pub fn parse(bytes: &[u8], max_rows: usize) -> Result<Table, String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!(
            "the file is larger than {} KiB; split it into smaller files",
            MAX_BYTES / 1024
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text".to_string())?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.contains('\0') {
        return Err("the file contains a NUL character; it is not a CSV text file".into());
    }
    let max_rows = max_rows.min(MAX_ROWS);
    let delimiter = detect_delimiter(text);
    let records = records(text, delimiter)?;
    let mut it = records.into_iter();
    let Some((hline, header)) = it.next() else {
        return Err("the file is empty: the first line must name the fields".into());
    };
    if header.len() > MAX_COLUMNS {
        return Err(format!("more than {MAX_COLUMNS} columns"));
    }
    let headers: Vec<String> = header.iter().map(|h| h.trim().to_string()).collect();
    for (i, h) in headers.iter().enumerate() {
        if h.is_empty() {
            return Err(format!("line {hline}: column {} has no name", i + 1));
        }
        if headers[..i].contains(h) {
            return Err(format!("line {hline}: the column {h:?} appears twice"));
        }
    }
    let mut rows = Vec::new();
    let mut lines = Vec::new();
    for (line, cells) in it {
        if cells.len() != headers.len() {
            return Err(format!(
                "line {line}: {} column(s), but the header has {}",
                cells.len(),
                headers.len()
            ));
        }
        if rows.len() >= max_rows {
            return Err(format!(
                "more than {max_rows} rows; split the file (at most {max_rows} rows per import)"
            ));
        }
        rows.push(cells);
        lines.push(line);
    }
    Ok(Table {
        delimiter,
        headers,
        rows,
        lines,
    })
}

/// `;` when the first record has more semicolons than commas outside quotes, else `,`.
fn detect_delimiter(text: &str) -> char {
    let (mut commas, mut semis, mut quoted) = (0usize, 0usize, false);
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            '\n' | '\r' if !quoted => break,
            ',' if !quoted => commas += 1,
            ';' if !quoted => semis += 1,
            _ => {}
        }
    }
    if semis > commas { ';' } else { ',' }
}

/// All non-empty records with the line each starts on.
fn records(text: &str, delim: char) -> Result<Vec<(usize, Vec<String>)>, String> {
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    let mut chars = text.chars().peekable();
    let mut line = 1usize;
    let mut record_line = 1usize;
    let mut cells: Vec<String> = Vec::new();
    let mut cell = String::new();
    // `at_start`: nothing of the current cell has been read; `quoted_cell`: it began with a quote.
    let mut at_start = true;
    let mut seen_any = false;
    macro_rules! end_cell {
        () => {{
            if cell.len() > MAX_CELL_BYTES {
                return Err(format!(
                    "line {record_line}: a cell is longer than {} KiB",
                    MAX_CELL_BYTES / 1024
                ));
            }
            cells.push(std::mem::take(&mut cell));
            at_start = true;
        }};
    }
    macro_rules! end_record {
        () => {{
            end_cell!();
            // A blank line is one empty cell: skip it.
            if !(cells.len() == 1 && cells[0].is_empty()) {
                out.push((record_line, std::mem::take(&mut cells)));
                if out.len() > MAX_ROWS + 1 {
                    return Err(format!("more than {MAX_ROWS} rows"));
                }
            } else {
                cells.clear();
            }
            seen_any = false;
        }};
    }
    while let Some(c) = chars.next() {
        if !seen_any {
            record_line = line;
            seen_any = true;
        }
        match c {
            '"' if at_start => {
                // A quoted cell.
                at_start = false;
                loop {
                    match chars.next() {
                        None => {
                            return Err(format!(
                                "line {record_line}: a quote is opened but never closed"
                            ));
                        }
                        Some('"') => {
                            if chars.peek() == Some(&'"') {
                                chars.next();
                                cell.push('"');
                            } else {
                                break;
                            }
                        }
                        Some(ch) => {
                            if ch == '\n' {
                                line += 1;
                            }
                            cell.push(ch);
                            if cell.len() > MAX_CELL_BYTES {
                                return Err(format!(
                                    "line {record_line}: a cell is longer than {} KiB",
                                    MAX_CELL_BYTES / 1024
                                ));
                            }
                        }
                    }
                }
                match chars.peek() {
                    None | Some('\n' | '\r') => {}
                    Some(&d) if d == delim => {}
                    Some(_) => {
                        return Err(format!(
                            "line {line}: text after the closing quote of a cell"
                        ));
                    }
                }
            }
            '"' => {
                return Err(format!(
                    "line {line}: a quote inside an unquoted cell; wrap the cell in quotes and double the quote"
                ));
            }
            c if c == delim => end_cell!(),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_record!();
                line += 1;
            }
            '\n' => {
                end_record!();
                line += 1;
            }
            c => {
                at_start = false;
                cell.push(c);
            }
        }
    }
    if seen_any {
        // The last record has no line break after it.
        if cell.len() > MAX_CELL_BYTES {
            return Err(format!(
                "line {record_line}: a cell is longer than {} KiB",
                MAX_CELL_BYTES / 1024
            ));
        }
        cells.push(cell);
        if !(cells.len() == 1 && cells[0].is_empty()) {
            out.push((record_line, cells));
        }
        if out.len() > MAX_ROWS + 1 {
            return Err(format!("more than {MAX_ROWS} rows"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Table, String> {
        parse(s.as_bytes(), MAX_ROWS)
    }

    #[test]
    fn plain_quoted_and_multiline_cells() {
        let t = p("name,note\r\nAda,\"says \"\"hi\"\"\"\nBob,\"two\nlines\"\n").unwrap();
        assert_eq!(t.headers, ["name", "note"]);
        assert_eq!(t.rows[0], ["Ada", "says \"hi\""]);
        assert_eq!(t.rows[1], ["Bob", "two\nlines"]);
        assert_eq!(t.lines, [2, 3]);
        assert_eq!(t.delimiter, ',');
    }

    #[test]
    fn a_byte_order_mark_is_dropped_and_semicolons_are_detected() {
        let t = parse("\u{feff}a;b\n1;2\n".as_bytes(), 10).unwrap();
        assert_eq!(
            (t.delimiter, t.headers.clone()),
            (';', vec!["a".into(), "b".into()])
        );
        assert_eq!(t.rows[0], ["1", "2"]);
        // Commas inside quotes do not decide, and the tab is not a delimiter.
        let t = p("\"a;b\",c\n1,2\n").unwrap();
        assert_eq!(t.delimiter, ',');
        let t = p("a\tb\n1\t2\n").unwrap();
        assert_eq!(t.headers, ["a\tb"]);
    }

    #[test]
    fn blank_lines_and_a_missing_final_newline() {
        let t = p("a,b\n\n1,2\n\n3,4").unwrap();
        assert_eq!(t.rows, [["1", "2"], ["3", "4"]]);
        assert_eq!(t.lines, [3, 5]);
        let t = p("a\n,\n").unwrap_err();
        assert!(t.contains("2 column"), "{t}");
    }

    #[test]
    fn structure_errors_name_the_line() {
        assert!(p("").unwrap_err().contains("empty"));
        assert!(p("a,b\n1\n").unwrap_err().contains("line 2"));
        assert!(p("a,a\n1,2\n").unwrap_err().contains("twice"));
        assert!(p("a,\n1,2\n").unwrap_err().contains("no name"));
        assert!(p("a,b\n1,\"x\n").unwrap_err().contains("never closed"));
        assert!(p("a,b\n1,x\"y\n").unwrap_err().contains("quote inside"));
        assert!(
            p("a,b\n1,\"x\"y\n")
                .unwrap_err()
                .contains("after the closing")
        );
        assert!(parse(b"a\n\xff\n", 5).unwrap_err().contains("UTF-8"));
        assert!(parse(b"a\n\0\n", 5).unwrap_err().contains("NUL"));
    }

    #[test]
    fn limits_hold() {
        let rows: String = (0..11).map(|i| format!("{i}\n")).collect();
        assert!(
            parse(format!("n\n{rows}").as_bytes(), 10)
                .unwrap_err()
                .contains("more than 10")
        );
        assert!(parse(format!("n\n{rows}").as_bytes(), 11).is_ok());
        let big = format!("a\n{}\n", "x".repeat(MAX_CELL_BYTES + 1));
        assert!(p(&big).unwrap_err().contains("longer"));
        let huge = vec![b'a'; MAX_BYTES + 1];
        assert!(parse(&huge, 5).unwrap_err().contains("larger"));
        let many = (0..MAX_COLUMNS + 1)
            .map(|i| format!("c{i}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(p(&many).unwrap_err().contains("columns"));
    }

    #[test]
    fn formula_looking_cells_are_kept_as_written() {
        let t = p("a\n=1+1\n@x\n-3\n").unwrap();
        assert_eq!(t.rows, [["=1+1"], ["@x"], ["-3"]]);
    }
}
