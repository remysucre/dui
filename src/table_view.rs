use crate::bridge::{self, TableData};
use crate::db::Db;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// Structural mutation collected during a UI pass.
enum TableAction {
    AddColumn,
    DropColumn(usize),
    RenameColumn(usize, String, String), // (col_index, old_name, new_name)
    AddRow,
    DeleteRow(i64),
}

/// State for a single table window.
pub struct TableWindow {
    id: usize,
    pub name: String,
    /// Stored when renaming starts so we can ALTER TABLE from old -> new
    pub rename_old: Option<String>,
    pub data: TableData,
    pub open: bool,
    pub renaming: bool,
    /// Which cell is currently being edited: (row, col)
    editing_cell: Option<(usize, usize)>,
    /// Column being renamed: (col_index, old_name)
    editing_col: Option<(usize, String)>,
    /// Pending async batch operation: (stmts, final_query)
    pending_batch: Option<(Vec<String>, String)>,
}

static NEXT_TABLE_WINDOW_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

impl TableWindow {
    pub fn new(name: String, data: TableData) -> Self {
        Self {
            id: NEXT_TABLE_WINDOW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            name,
            rename_old: None,
            data,
            open: true,
            renaming: false,
            editing_cell: None,
            editing_col: None,
            pending_batch: None,
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

        total + scroll_bar + 20.0 // window margins
    }

    /// Render this table as a floating egui::Window. Returns false if closed.
    pub fn show(&mut self, ctx: &egui::Context, db: &dyn Db) -> bool {
        // Poll pending async batch operation
        if let Some((stmts, query)) = self.pending_batch.clone() {
            match db.batch(&stmts, Some(&query)) {
                Ok(result) if !result.columns.is_empty() => {
                    self.data = bridge::parse_rowid_result(result);
                    self.pending_batch = None;
                }
                Ok(_) => { ctx.request_repaint(); }
                Err(_) => { self.pending_batch = None; }
            }
        }

        let mut open = self.open;
        let width = self.estimate_width(ctx);

        // Collect pending edits and structural actions outside the UI closure
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        let mut actions: Vec<TableAction> = Vec::new();
        let mut new_editing: Option<(usize, usize)> = self.editing_cell;
        let editing = self.editing_cell;
        let mut new_editing_col: Option<(usize, String)> = self.editing_col.clone();
        let editing_col = self.editing_col.clone();

        let window_frame = egui::Frame::window(&ctx.style())
            .inner_margin(egui::Margin::same(2));
        egui::Window::new(&self.name)
            .id(egui::Id::new(("table_window", self.id)))
            .open(&mut open)
            .default_width(width)
            .resizable(true)
            .collapsible(true)
            .frame(window_frame)
            .show(ctx, |ui| {
                let row_count = self.data.rows.len();
                ui.label(format!("{} rows", row_count));
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
                table = table.column(Column::auto().resizable(false));

                table
                    .header(20.0, |mut header| {
                        for ci in 0..self.data.columns.len() {
                            header.col(|ui| {
                                if editing_col.as_ref().map_or(false, |(idx, _)| *idx == ci) {
                                    // Inline rename editor
                                    let col = &mut self.data.columns[ci];
                                    let resp = ui.text_edit_singleline(col);
                                    if !resp.has_focus() && !resp.lost_focus() {
                                        resp.request_focus();
                                    }
                                    if resp.lost_focus() {
                                        let old = editing_col.as_ref().unwrap().1.clone();
                                        let new = col.clone();
                                        if old != new {
                                            actions.push(TableAction::RenameColumn(ci, old, new));
                                        }
                                        new_editing_col = None;
                                    }
                                } else {
                                    let col_name = &self.data.columns[ci];
                                    let label_resp = ui.strong(col_name);
                                    let header_resp = ui.interact(ui.max_rect(), ui.id().with(("col_header", ci)), egui::Sense::click());
                                    if label_resp.double_clicked() || header_resp.double_clicked() {
                                        new_editing_col = Some((ci, col_name.clone()));
                                    }
                                    let hovered = ui.rect_contains_pointer(ui.max_rect());
                                    let x_resp = ui.add_visible(hovered, egui::Button::new(egui_phosphor::regular::X).small().fill(egui::Color32::from_rgb(255, 180, 180)));
                                    if x_resp.clicked() {
                                        actions.push(TableAction::DropColumn(ci));
                                    }
                                    header_resp.context_menu(|ui| {
                                        if ui.button("Rename column").clicked() {
                                            new_editing_col = Some((ci, self.data.columns[ci].clone()));
                                            ui.close_menu();
                                        }
                                        if ui.button("Delete column").clicked() {
                                            actions.push(TableAction::DropColumn(ci));
                                            ui.close_menu();
                                        }
                                        if ui.button("Add column").clicked() {
                                            actions.push(TableAction::AddColumn);
                                            ui.close_menu();
                                        }
                                    });
                                }
                            });
                        }
                        header.col(|ui| {
                            if ui.small_button(format!("{} col", egui_phosphor::regular::PLUS)).clicked() {
                                actions.push(TableAction::AddColumn);
                            }
                        });
                    })
                    .body(|body| {
                        body.rows(text_height, row_count, |mut row| {
                            let ri = row.index();
                            let col_count = self.data.columns.len();
                            let mut row_hovered = false;
                            for ci in 0..col_count {
                                row.col(|ui| {
                                    if ui.rect_contains_pointer(ui.max_rect()) {
                                        row_hovered = true;
                                    }
                                    if editing == Some((ri, ci)) {
                                        let cell = &mut self.data.rows[ri][ci];
                                        let resp = ui.text_edit_singleline(cell);
                                        if !resp.has_focus() && !resp.lost_focus() {
                                            resp.request_focus();
                                        }
                                        if resp.changed() {
                                            edits.push((ri, ci, cell.clone()));
                                        }
                                        if resp.lost_focus() {
                                            new_editing = None;
                                        }
                                    } else {
                                        let cell = &self.data.rows[ri][ci];
                                        ui.label(cell);
                                        let cell_resp = ui.interact(ui.max_rect(), ui.id().with(("cell", ri, ci)), egui::Sense::click());
                                        if cell_resp.double_clicked() {
                                            new_editing = Some((ri, ci));
                                        }
                                        cell_resp.context_menu(|ui| {
                                            if ui.button("Add row").clicked() {
                                                actions.push(TableAction::AddRow);
                                                ui.close_menu();
                                            }
                                            if let Some(&rid) = self.data.row_ids.get(ri) {
                                                if ui.button("Delete row").clicked() {
                                                    actions.push(TableAction::DeleteRow(rid));
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                    }
                                });
                            }
                            row.col(|ui| {
                                if ui.rect_contains_pointer(ui.max_rect()) {
                                    row_hovered = true;
                                }
                                if let Some(&rid) = self.data.row_ids.get(ri) {
                                    let x_resp = ui.add_visible(row_hovered, egui::Button::new(egui_phosphor::regular::X).small().fill(egui::Color32::from_rgb(255, 180, 180)));
                                    if x_resp.clicked() {
                                        actions.push(TableAction::DeleteRow(rid));
                                    }
                                }
                            });
                        });
                    });
                if ui.small_button(format!("{} row", egui_phosphor::regular::PLUS)).clicked() {
                    actions.push(TableAction::AddRow);
                }
            });

        self.editing_cell = new_editing;
        self.editing_col = new_editing_col;

        // Apply cell edits
        for (ri, ci, new_val) in edits {
            if let Some(&rid) = self.data.row_ids.get(ri) {
                let col = &self.data.columns[ci];
                let escaped = new_val.replace('\'', "''");
                let sql = format!(
                    "UPDATE \"{}\" SET \"{}\" = '{}' WHERE rowid = {}",
                    self.name, col, escaped, rid
                );
                let _ = db.execute(&sql);
            }
        }

        // Execute structural actions using batch to preserve ordering
        for action in actions {
            let mutation_sql = match &action {
                TableAction::AddColumn => {
                    let new_col = format!("col_{}", self.data.columns.len());
                    format!(
                        "ALTER TABLE \"{}\" ADD COLUMN \"{}\" VARCHAR",
                        self.name, new_col
                    )
                }
                TableAction::DropColumn(ci) => {
                    if let Some(col) = self.data.columns.get(*ci) {
                        format!(
                            "ALTER TABLE \"{}\" DROP COLUMN \"{}\"",
                            self.name, col
                        )
                    } else {
                        continue;
                    }
                }
                TableAction::RenameColumn(_ci, old, new) => {
                    format!(
                        "ALTER TABLE \"{}\" RENAME COLUMN \"{}\" TO \"{}\"",
                        self.name, old, new
                    )
                }
                TableAction::AddRow => {
                    format!("INSERT INTO \"{}\" DEFAULT VALUES", self.name)
                }
                TableAction::DeleteRow(rid) => {
                    format!(
                        "DELETE FROM \"{}\" WHERE rowid = {}",
                        self.name, rid
                    )
                }
            };

            // Use batch: run mutation then re-query the table
            let refresh_query = format!(
                "SELECT rowid, * FROM \"{}\" LIMIT 10000",
                self.name
            );
            let stmts = vec![mutation_sql];
            match db.batch(&stmts, Some(&refresh_query)) {
                Ok(result) if !result.columns.is_empty() => {
                    self.data = bridge::parse_rowid_result(result);
                }
                Ok(_) => {
                    // WASM: result pending, poll next frame
                    self.pending_batch = Some((stmts, refresh_query));
                }
                Err(_) => {}
            }
        }

        self.open = open;
        open
    }
}
