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

    /// Render the query window. Returns false if closed.
    pub fn show(&mut self, ctx: &egui::Context, conn: &Connection) -> bool {
        let mut open = self.open;
        egui::Window::new(format!("Query {}", self.id))
            .open(&mut open)
            .default_size([600.0, 400.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("SQL:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.query)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );

                if ui.button("Run").clicked() {
                    self.result = Some(run_query(conn, &self.query));
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
        open
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
