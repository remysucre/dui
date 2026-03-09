use crate::bridge::{self, TableData};
use crate::db::Db;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// State for a single table window.
pub struct TableWindow {
    id: usize,
    pub name: String,
    /// Stored when renaming starts so we can ALTER TABLE from old -> new
    pub rename_old: Option<String>,
    pub data: TableData,
    pub open: bool,
    pub renaming: bool,
    /// Pending async batch operation: (stmts, final_query)
    pending_batch: Option<(Vec<String>, String)>,
    /// Column names as last committed to DB (for detecting renames)
    committed_columns: Vec<String>,
}

static NEXT_TABLE_WINDOW_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

impl TableWindow {
    pub fn new(name: String, data: TableData) -> Self {
        let committed_columns = data.columns.clone();
        Self {
            id: NEXT_TABLE_WINDOW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            name,
            rename_old: None,
            data,
            open: true,
            renaming: false,
            pending_batch: None,
            committed_columns,
        }
    }

    /// Start renaming -- saves old name for the ALTER TABLE.
    pub fn start_rename(&mut self) {
        self.rename_old = Some(self.name.clone());
        self.renaming = true;
    }

    /// Finish renaming -- issues ALTER TABLE if name changed.
    pub fn finish_rename(&mut self, db: &dyn Db) -> Result<(), String> {
        self.renaming = false;
        if let Some(old) = self.rename_old.take() {
            if old != self.name {
                let sql = format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", old, self.name);
                db.execute(&sql)
                    .map_err(|e| format!("Rename failed: {e}"))?;
            }
        }
        Ok(())
    }

    pub fn refresh(&mut self, db: &dyn Db) {
        if self.pending_batch.is_some() {
            return;
        }
        let query = format!("SELECT rowid, * FROM \"{}\" LIMIT 10000", self.name);
        match db.batch(&[], Some(&query)) {
            Ok(result) if !result.columns.is_empty() => {
                self.data = bridge::parse_rowid_result(result);
            }
            Ok(_) => {
                self.pending_batch = Some((vec![], query));
            }
            Err(_) => {}
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

        total + scroll_bar + 20.0
    }

    /// Render this table as a floating egui::Window. Returns false if closed.
    pub fn show(&mut self, ctx: &egui::Context, db: &dyn Db) -> bool {
        // Poll pending async batch operation
        if let Some((stmts, query)) = self.pending_batch.clone() {
            match db.batch(&stmts, Some(&query)) {
                Ok(result) if !result.columns.is_empty() => {
                    self.data = bridge::parse_rowid_result(result);
                    self.committed_columns = self.data.columns.clone();
                    self.pending_batch = None;
                }
                Ok(_) => { ctx.request_repaint_after(std::time::Duration::from_millis(16)); }
                Err(_) => { self.pending_batch = None; }
            }
        }

        let mut open = self.open;
        let width = self.estimate_width(ctx);
        let mut renames: Vec<(String, String)> = Vec::new();

        let window_frame = egui::Frame::window(&ctx.style())
            .inner_margin(egui::Margin::same(2));
        egui::Window::new(&self.name)
            .id(egui::Id::new(("table_window", self.id)))
            .open(&mut open)
            .default_width(width.min(800.0))
            .resizable(true)
            .collapsible(true)
            .frame(window_frame)
            .show(ctx, |ui| {
                let row_count = self.data.rows.len();

                let text_height = egui::TextStyle::Body
                    .resolve(ui.style())
                    .size
                    .max(ui.spacing().interact_size.y);

                egui::ScrollArea::horizontal().show(ui, |ui| {
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .min_scrolled_height(0.0)
                    .max_scroll_height(400.0)
                    .auto_shrink([true, false]);

                for _ in &self.data.columns {
                    table = table.column(Column::auto().at_least(40.0).at_most(300.0).resizable(true).clip(true));
                }
                table
                    .header(20.0, |mut header| {
                        for ci in 0..self.data.columns.len() {
                            header.col(|ui| {
                                let col = &mut self.data.columns[ci];
                                let resp = ui.text_edit_singleline(col);
                                if resp.lost_focus() {
                                    let old = &self.committed_columns[ci];
                                    if col != old {
                                        renames.push((old.clone(), col.clone()));
                                    }
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(text_height, row_count, |mut row| {
                            let ri = row.index();
                            let col_count = self.data.columns.len();
                            for ci in 0..col_count {
                                row.col(|ui| {
                                    let cell = &self.data.rows[ri][ci];
                                    ui.label(cell);
                                });
                            }
                        });
                    });
                }); // end ScrollArea
                ui.label(format!("{} rows", row_count));
            });

        // Issue column renames
        for (old, new) in renames {
            let sql = format!(
                "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
                self.name, old, new
            );
            let refresh_query = format!(
                "SELECT rowid, * FROM \"{}\" LIMIT 10000",
                self.name
            );
            let stmts = vec![sql];
            match db.batch(&stmts, Some(&refresh_query)) {
                Ok(result) if !result.columns.is_empty() => {
                    self.data = bridge::parse_rowid_result(result);
                    self.committed_columns = self.data.columns.clone();
                }
                Ok(_) => {
                    self.pending_batch = Some((stmts, refresh_query));
                }
                Err(_) => {
                    // Revert the column name on failure
                    self.data.columns = self.committed_columns.clone();
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        self.open = open;
        open
    }
}
