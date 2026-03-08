use crate::db::Db;

/// Parsed table data from a file.
#[derive(Debug, Clone)]
pub struct TableData {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_ids: Vec<i64>,
}

/// Load a file into DuckDB and return the table name and parsed data.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_file(db: &dyn Db, path: &str) -> Result<(String, TableData), String> {
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("table");

    // Derive a safe table name from the file name (strip extension, replace non-alnum)
    let safe_name: String = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("table")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    // Deduplicate: if the table already exists, append a suffix
    let table_name = dedup_table_name(db, &safe_name);

    // Ingest the file
    db.execute(&format!(
        "CREATE TABLE \"{table_name}\" AS SELECT * FROM read_csv_auto('{path}')"
    ))
    .map_err(|e| format!("Failed to load file: {e}"))?;

    read_table(db, &table_name).map(|data| (table_name, data))
}

/// Load CSV bytes directly (used in WASM where we only have bytes, not file paths).
pub fn load_csv_bytes(db: &dyn Db, name_hint: &str, csv: &str) -> Result<(String, TableData), String> {
    let safe_name: String = name_hint
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    let safe_name = if safe_name.is_empty() { "table".to_string() } else { safe_name };

    let table_name = dedup_table_name(db, &safe_name);

    // Parse CSV header + rows manually
    let mut lines = csv.lines();
    let header = lines.next().ok_or("CSV is empty")?;
    let columns: Vec<&str> = header.split(',').map(|s| s.trim()).collect();

    // Create table
    let col_defs: Vec<String> = columns.iter().map(|c| format!("\"{}\" VARCHAR", c)).collect();
    db.execute(&format!(
        "CREATE TABLE \"{}\" ({})",
        table_name,
        col_defs.join(", ")
    ))
    .map_err(|e| format!("Failed to create table: {e}"))?;

    // Insert rows
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let vals: Vec<String> = line
            .split(',')
            .map(|s| {
                let s = s.trim();
                format!("'{}'", s.replace('\'', "''"))
            })
            .collect();
        db.execute(&format!(
            "INSERT INTO \"{}\" VALUES ({})",
            table_name,
            vals.join(", ")
        ))
        .map_err(|e| format!("Failed to insert row: {e}"))?;
    }

    read_table(db, &table_name).map(|data| (table_name, data))
}

fn dedup_table_name(db: &dyn Db, safe_name: &str) -> String {
    let mut name = safe_name.to_string();
    let mut suffix = 1u32;
    loop {
        let exists = db
            .query(&format!(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = '{name}'"
            ))
            .map(|r| {
                r.rows.first()
                    .and_then(|row| row.first())
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0) > 0
            })
            .unwrap_or(false);
        if !exists {
            break name;
        }
        suffix += 1;
        name = format!("{safe_name}_{suffix}");
    }
}

/// Read all data from a table (columns + rows with rowid).
pub fn read_table(db: &dyn Db, table_name: &str) -> Result<TableData, String> {
    // Get column names
    let col_result = db.query(&format!("PRAGMA table_info('{table_name}')"))?;
    let columns: Vec<String> = col_result
        .rows
        .iter()
        .filter_map(|row| row.get(1).cloned())
        .collect();

    // Read rows with rowid
    let col_list: String = columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let data_result = db.query(&format!(
        "SELECT rowid, {col_list} FROM \"{table_name}\" LIMIT 10000"
    ))?;

    let mut rows = Vec::new();
    let mut row_ids = Vec::new();
    for row in &data_result.rows {
        if let Some(rid_str) = row.first() {
            let rid: i64 = rid_str.parse().unwrap_or(0);
            row_ids.push(rid);
            rows.push(row[1..].to_vec());
        }
    }

    Ok(TableData { columns, rows, row_ids })
}

/// Create an empty table with a single column.
pub fn create_empty_table(db: &dyn Db, name: &str) -> Result<TableData, String> {
    db.execute(&format!(
        "CREATE TABLE \"{name}\" (value VARCHAR)"
    ))
    .map_err(|e| format!("Failed to create table: {e}"))?;
    Ok(TableData {
        columns: vec!["value".to_string()],
        rows: Vec::new(),
        row_ids: Vec::new(),
    })
}

/// Drop a table from DuckDB.
pub fn drop_table(db: &dyn Db, name: &str) -> Result<(), String> {
    db.execute(&format!("DROP TABLE IF EXISTS \"{name}\""))
        .map_err(|e| format!("Failed to drop table: {e}"))
}
