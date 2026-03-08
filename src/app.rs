use crate::bridge;
use crate::query_window::QueryWindow;
use crate::table_view::TableWindow;
use duckdb::Connection;
use eframe::egui;

pub struct DuiApp {
    conn: Connection,
    tables: Vec<TableWindow>,
    next_table_id: usize,
    query_windows: Vec<QueryWindow>,
    error: Option<String>,
}

impl DuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        Self {
            conn: Connection::open_in_memory().expect("Failed to open DuckDB"),
            tables: Vec::new(),
            next_table_id: 1,
            query_windows: Vec::new(),
            error: None,
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files: Vec<egui::DroppedFile> =
            ctx.input(|i| i.raw.dropped_files.clone());

        for file in dropped_files {
            let path = if let Some(path) = &file.path {
                match path.to_str() {
                    Some(p) => p.to_string(),
                    None => {
                        self.error = Some("Invalid file path".to_string());
                        continue;
                    }
                }
            } else {
                continue;
            };

            match bridge::load_file(&self.conn, &path) {
                Ok((name, data)) => {
                    self.tables.push(TableWindow::new(name, data));
                }
                Err(e) => {
                    self.error = Some(e);
                }
            }
        }
    }
}

impl eframe::App for DuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ctx);

        // Render all table windows (keep closed ones for the side panel)
        for tw in &mut self.tables {
            tw.show(ctx, &self.conn);
        }

        // Render all query windows
        let mut any_query_ran = false;
        for qw in &mut self.query_windows {
            let ran = qw.show(ctx, &self.conn);
            if ran {
                any_query_ran = true;
            }
        }

        // Refresh table data after a query modifies the database
        if any_query_ran {
            for tw in &mut self.tables {
                tw.refresh(&self.conn);
            }
        }

        // Right side panel
        egui::SidePanel::right("tables_pane")
            .default_width(160.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Tables");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(egui_phosphor::regular::PLUS).clicked() {
                        let id = self.next_table_id;
                        self.next_table_id += 1;
                        let name = format!("table_{id}");
                        match bridge::create_empty_table(&self.conn, &name) {
                            Ok(data) => self.tables.push(TableWindow::new(name, data)),
                            Err(e) => self.error = Some(e),
                        }
                    }
                    });
                });
                ui.separator();
                let mut remove_table_idx = None;
                let mut finish_rename_idx = None;
                for (idx, tw) in self.tables.iter_mut().enumerate() {
                    if tw.renaming {
                        let resp = ui.text_edit_singleline(&mut tw.name);
                        if resp.lost_focus() {
                            finish_rename_idx = Some(idx);
                        } else {
                            resp.request_focus();
                        }
                    } else {
                        ui.horizontal(|ui| {
                            let label = ui.selectable_label(tw.open, &tw.name);
                            if label.clicked() {
                                tw.open = !tw.open;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_table_idx = Some(idx);
                                }
                                if ui.small_button(egui_phosphor::regular::PENCIL_SIMPLE).clicked() {
                                    tw.start_rename();
                                }
                            });
                        });
                    }
                }
                if let Some(idx) = finish_rename_idx {
                    if let Err(e) = self.tables[idx].finish_rename(&self.conn) {
                        self.error = Some(e);
                    }
                }
                if let Some(idx) = remove_table_idx {
                    let name = &self.tables[idx].name;
                    if let Err(e) = bridge::drop_table(&self.conn, name) {
                        self.error = Some(e);
                    }
                    self.tables.remove(idx);
                }
                if self.tables.is_empty() {
                    ui.weak("No tables loaded");
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading("Queries");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(egui_phosphor::regular::PLUS).clicked() {
                            let id = self.query_windows.len() + 1;
                            self.query_windows.push(QueryWindow::new(id));
                        }
                    });
                });
                ui.separator();
                let mut remove_idx = None;
                for (idx, qw) in self.query_windows.iter_mut().enumerate() {
                    if qw.renaming {
                        let resp = ui.text_edit_singleline(&mut qw.name);
                        if resp.lost_focus() {
                            qw.renaming = false;
                        } else {
                            resp.request_focus();
                        }
                    } else {
                        ui.horizontal(|ui| {
                            let label = ui.selectable_label(qw.open, &qw.name);
                            if label.clicked() {
                                qw.open = !qw.open;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button(egui_phosphor::regular::X).clicked() {
                                    remove_idx = Some(idx);
                                }
                                if ui.small_button(egui_phosphor::regular::PENCIL_SIMPLE).clicked() {
                                    qw.renaming = true;
                                }
                            });
                        });
                    }
                }
                if let Some(idx) = remove_idx {
                    self.query_windows.remove(idx);
                }
                if self.query_windows.is_empty() {
                    ui.weak("No queries");
                }
            });

        let has_tables = self.tables.iter().any(|tw| tw.open);

        // Central panel: drop zone hint when no tables are open
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(err) = &self.error.clone() {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), format!("Error: {err}"));
                if ui.button("Dismiss").clicked() {
                    self.error = None;
                }
                ui.separator();
            }

            if !has_tables {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.heading("dui");
                    ui.add_space(8.0);
                    ui.label("Drop a data file here");
                });
            }
        });

        preview_files_being_dropped(ctx);
    }
}

/// Paints a semi-transparent overlay when files are being dragged over the window.
fn preview_files_being_dropped(ctx: &egui::Context) {
    use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};

    if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
        let painter =
            ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

        let screen_rect = ctx.screen_rect();
        painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(160));
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            "Drop file to load",
            TextStyle::Heading.resolve(&ctx.style()),
            Color32::WHITE,
        );
    }
}
