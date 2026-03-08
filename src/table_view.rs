use crate::bridge::TableData;
use duckdb::Connection;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// State for a single table window.
pub struct TableWindow {
    pub name: String,
    pub data: TableData,
    pub open: bool,
}

impl TableWindow {
    pub fn new(name: String, data: TableData) -> Self {
        Self {
            name,
            data,
            open: true,
        }
    }

    pub fn refresh(&mut self, conn: &Connection) {
        let sql = format!("SELECT * FROM \"{}\" LIMIT 10000", self.name);
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(mut result) = stmt.query([]) {
                let col_count = result.as_ref().unwrap().column_count();
                let columns: Vec<String> = (0..col_count)
                    .map(|i| {
                        result
                            .as_ref()
                            .unwrap()
                            .column_name(i)
                            .map_or("?".to_string(), |v| v.to_string())
                    })
                    .collect();

                let mut rows = Vec::new();
                while let Ok(Some(row)) = result.next() {
                    let mut vals = Vec::with_capacity(col_count);
                    for i in 0..col_count {
                        let val: String = row
                            .get::<_, duckdb::types::Value>(i)
                            .map(|v| crate::bridge::format_value(&v))
                            .unwrap_or_default();
                        vals.push(val);
                    }
                    rows.push(vals);
                }

                self.data = TableData { columns, rows };
            }
        }
    }

    fn estimate_width(&self, ctx: &egui::Context) -> f32 {
        let font_id = egui::TextStyle::Body.resolve(&ctx.style());
        let char_width = ctx.fonts(|f| f.glyph_width(&font_id, '0'));
        let padding_per_col = 16.0;
        let scroll_bar = 20.0;

        let total: f32 = self
            .data
            .columns
            .iter()
            .enumerate()
            .map(|(ci, header)| {
                let mut max_len = header.len();
                for row in self.data.rows.iter().take(100) {
                    if let Some(cell) = row.get(ci) {
                        max_len = max_len.max(cell.len());
                    }
                }
                (max_len as f32 * char_width + padding_per_col).min(300.0).max(40.0)
            })
            .sum();

        total + scroll_bar + 20.0 // window margins
    }

    /// Render this table as a floating egui::Window. Returns false if closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let mut open = self.open;
        let width = self.estimate_width(ctx);
        egui::Window::new(&self.name)
            .open(&mut open)
            .default_width(width)
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                let row_count = self.data.rows.len();
                ui.label(format!(
                    "{} columns, {} rows",
                    self.data.columns.len(),
                    row_count
                ));
                ui.separator();

                let text_height = egui::TextStyle::Body
                    .resolve(ui.style())
                    .size
                    .max(ui.spacing().interact_size.y);

                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .min_scrolled_height(0.0);

                for _ in &self.data.columns {
                    table = table.column(Column::auto().at_least(40.0).resizable(true));
                }

                table
                    .header(20.0, |mut header| {
                        for col_name in &self.data.columns {
                            header.col(|ui| {
                                ui.strong(col_name);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(text_height, row_count, |mut row| {
                            let row_data = &self.data.rows[row.index()];
                            for cell in row_data {
                                row.col(|ui| {
                                    ui.label(cell);
                                });
                            }
                        });
                    });
            });
        self.open = open;
        open
    }
}
