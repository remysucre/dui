use crate::bridge::TableData;
use duckdb::Connection;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

pub struct QueryWindow {
    id: usize,
    query: String,
    result: Option<Result<TableData, String>>,
    open: bool,
}

impl QueryWindow {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            query: String::new(),
            result: None,
            open: true,
        }
    }

    /// Render the query window. Returns (still_open, query_was_run).
    pub fn show(&mut self, ctx: &egui::Context, conn: &Connection) -> (bool, bool) {
        let mut open = self.open;
        let ran = std::cell::Cell::new(false);
        egui::Window::new(format!("Query {}", self.id))
            .open(&mut open)
            .default_size([600.0, 400.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("SQL:");
                let layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                    let layout_job = highlight_sql(ui, text, wrap_width);
                    ui.fonts(|f| f.layout_job(layout_job))
                };
                ui.add(
                    egui::TextEdit::multiline(&mut self.query)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .layouter(&mut layouter.clone()),
                );

                if ui.button("Run").clicked() {
                    self.result = Some(run_query(conn, &self.query));
                    ran.set(true);
                }

                ui.separator();

                match &self.result {
                    Some(Ok(data)) => {
                        ui.label(format!(
                            "{} columns, {} rows",
                            data.columns.len(),
                            data.rows.len()
                        ));
                        show_table(ui, data);
                    }
                    Some(Err(e)) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 100, 100),
                            format!("Error: {e}"),
                        );
                    }
                    None => {}
                }
            });
        self.open = open;
        (open, ran.get())
    }
}

fn run_query(conn: &Connection, sql: &str) -> Result<TableData, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let mut result = stmt.query([]).map_err(|e| e.to_string())?;

    // Column info is available on Rows after execution
    let col_count = result.as_ref().unwrap().column_count();
    let columns: Vec<String> = (0..col_count)
        .map(|i| {
            result
                .as_ref()
                .unwrap()
                .column_name(i)
                .map_or("?", |v| v)
                .to_string()
        })
        .collect();

    let mut rows = Vec::new();
    while let Some(row) = result.next().map_err(|e| e.to_string())? {
        let mut vals = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let val: String = row
                .get::<_, duckdb::types::Value>(i)
                .map(|v| format_value(&v))
                .unwrap_or_default();
            vals.push(val);
        }
        rows.push(vals);
    }

    Ok(TableData { columns, rows })
}

fn format_value(v: &duckdb::types::Value) -> String {
    match v {
        duckdb::types::Value::Null => String::new(),
        duckdb::types::Value::Boolean(b) => b.to_string(),
        duckdb::types::Value::TinyInt(n) => n.to_string(),
        duckdb::types::Value::SmallInt(n) => n.to_string(),
        duckdb::types::Value::Int(n) => n.to_string(),
        duckdb::types::Value::BigInt(n) => n.to_string(),
        duckdb::types::Value::HugeInt(n) => n.to_string(),
        duckdb::types::Value::UTinyInt(n) => n.to_string(),
        duckdb::types::Value::USmallInt(n) => n.to_string(),
        duckdb::types::Value::UInt(n) => n.to_string(),
        duckdb::types::Value::UBigInt(n) => n.to_string(),
        duckdb::types::Value::Float(n) => n.to_string(),
        duckdb::types::Value::Double(n) => n.to_string(),
        duckdb::types::Value::Text(s) => s.clone(),
        _ => format!("{v:?}"),
    }
}

const SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP", "ALTER", "TABLE",
    "INTO", "VALUES", "SET", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "FULL", "CROSS", "ON",
    "AND", "OR", "NOT", "IN", "IS", "NULL", "AS", "ORDER", "BY", "GROUP", "HAVING", "LIMIT",
    "OFFSET", "UNION", "ALL", "DISTINCT", "BETWEEN", "LIKE", "ILIKE", "EXISTS", "CASE", "WHEN",
    "THEN", "ELSE", "END", "ASC", "DESC", "WITH", "RECURSIVE", "CAST", "TRUE", "FALSE", "COUNT",
    "SUM", "AVG", "MIN", "MAX", "OVER", "PARTITION", "WINDOW", "FILTER", "USING", "NATURAL",
    "EXCEPT", "INTERSECT", "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "INDEX", "VIEW",
    "REPLACE", "IF", "BEGIN", "COMMIT", "ROLLBACK", "PRAGMA", "DESCRIBE", "EXPLAIN", "ANALYZE",
];

fn highlight_sql(ui: &egui::Ui, text: &str, wrap_width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let mono = egui::TextFormat {
        font_id: egui::TextStyle::Monospace.resolve(ui.style()),
        ..Default::default()
    };

    let keyword_fmt = egui::TextFormat {
        color: egui::Color32::from_rgb(86, 156, 214), // blue
        ..mono.clone()
    };
    let string_fmt = egui::TextFormat {
        color: egui::Color32::from_rgb(206, 145, 120), // orange
        ..mono.clone()
    };
    let number_fmt = egui::TextFormat {
        color: egui::Color32::from_rgb(181, 206, 168), // green
        ..mono.clone()
    };
    let comment_fmt = egui::TextFormat {
        color: egui::Color32::from_rgb(106, 153, 85), // dim green
        ..mono.clone()
    };

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Line comment: --
        if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            job.append(&s, 0.0, comment_fmt.clone());
            continue;
        }

        // Block comment: /* ... */
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            let s: String = chars[start..i].iter().collect();
            job.append(&s, 0.0, comment_fmt.clone());
            continue;
        }

        // String literal: '...'
        if chars[i] == '\'' {
            let start = i;
            i += 1;
            while i < len {
                if chars[i] == '\'' {
                    i += 1;
                    if i < len && chars[i] == '\'' {
                        i += 1; // escaped quote
                    } else {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            job.append(&s, 0.0, string_fmt.clone());
            continue;
        }

        // Number
        if chars[i].is_ascii_digit()
            || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // Only highlight if not part of an identifier
            if start == 0
                || !(chars[start - 1].is_alphanumeric() || chars[start - 1] == '_')
            {
                let s: String = chars[start..i].iter().collect();
                job.append(&s, 0.0, number_fmt.clone());
            } else {
                let s: String = chars[start..i].iter().collect();
                job.append(&s, 0.0, mono.clone());
            }
            continue;
        }

        // Word (identifier or keyword)
        if chars[i].is_alphanumeric() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();
            if SQL_KEYWORDS.contains(&upper.as_str()) {
                job.append(&word, 0.0, keyword_fmt.clone());
            } else {
                job.append(&word, 0.0, mono.clone());
            }
            continue;
        }

        // Everything else (operators, whitespace, punctuation)
        let s: String = chars[i..i + 1].iter().collect();
        job.append(&s, 0.0, mono.clone());
        i += 1;
    }

    job
}

fn show_table(ui: &mut egui::Ui, data: &TableData) {
    let row_count = data.rows.len();
    let text_height = egui::TextStyle::Body
        .resolve(ui.style())
        .size
        .max(ui.spacing().interact_size.y);

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .min_scrolled_height(0.0);

    for _ in &data.columns {
        table = table.column(Column::auto().at_least(40.0).resizable(true));
    }

    table
        .header(20.0, |mut header| {
            for col_name in &data.columns {
                header.col(|ui| {
                    ui.strong(col_name);
                });
            }
        })
        .body(|body| {
            body.rows(text_height, row_count, |mut row| {
                let row_data = &data.rows[row.index()];
                for cell in row_data {
                    row.col(|ui| {
                        ui.label(cell);
                    });
                }
            });
        });
}
