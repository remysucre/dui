use que::bridge;
use que::db::{Db, NativeDb};
use que::query_window::QueryWindow;
use que::table_view::TableWindow;
use eframe::egui;

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

#[test]
fn empty_state() {
    let mut harness = egui_kittest::Harness::new_ui(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("¿Qué?");
            ui.add_space(8.0);
            ui.label("Drop a data file here");
        });
    });
    harness.fit_contents();
    harness.run();
    harness.snapshot("empty_state");
}

#[test]
fn table_with_data() {
    let db = NativeDb::new();
    db.execute("CREATE TABLE users (name VARCHAR, age INTEGER, active BOOLEAN)")
        .unwrap();
    db.execute("INSERT INTO users VALUES ('Alice', 30, true), ('Bob', 25, false), ('Charlie', 35, true)")
        .unwrap();
    let data = bridge::read_table(&db, "users").unwrap();
    let mut tw = TableWindow::new("users".to_string(), data);

    let mut harness = egui_kittest::Harness::new(|ctx| {
        setup_fonts(ctx);
        tw.show(ctx, &db);
    });
    harness.set_size(egui::vec2(500.0, 250.0));
    harness.run();
    harness.snapshot("table_with_data");
}

#[test]
fn query_window_empty() {
    let db = NativeDb::new();
    let mut qw = QueryWindow::new(1);

    let mut harness = egui_kittest::Harness::new(|ctx| {
        setup_fonts(ctx);
        qw.show(ctx, &db);
    });
    harness.set_size(egui::vec2(400.0, 300.0));
    harness.run();
    harness.snapshot("query_window_empty");
}

#[test]
fn load_csv_file() {
    let db = NativeDb::new();
    let csv_path = std::env::temp_dir().join("dui_test_snapshot.csv");
    std::fs::write(&csv_path, "name,score\nAlice,100\nBob,85\n").unwrap();

    let (name, data) = bridge::load_file(&db, csv_path.to_str().unwrap()).unwrap();
    let mut tw = TableWindow::new(name, data);

    let mut harness = egui_kittest::Harness::new(|ctx| {
        setup_fonts(ctx);
        tw.show(ctx, &db);
    });
    harness.set_size(egui::vec2(400.0, 200.0));
    harness.run();
    harness.snapshot("load_csv_file");

    std::fs::remove_file(csv_path).ok();
}
