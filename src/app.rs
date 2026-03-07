use crate::bridge;
use crate::table_view::TableWindow;
use duckdb::Connection;
use eframe::egui;

pub struct DuiApp {
    conn: Connection,
    tables: Vec<TableWindow>,
    error: Option<String>,
}

impl DuiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            conn: Connection::open_in_memory().expect("Failed to open DuckDB"),
            tables: Vec::new(),
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

        // Render all table windows
        self.tables.retain_mut(|tw| tw.show(ctx));

        let has_tables = !self.tables.is_empty();

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
