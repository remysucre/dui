use duckdb::Connection;

/// Parsed table data from a file.
#[derive(Debug, Clone)]
pub struct TableData {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Load a file into DuckDB and return the table name and parsed data.
pub fn load_file(conn: &Connection, path: &str) -> Result<(String, TableData), String> {
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
    let table_name = {
        let mut name = safe_name.clone();
        let mut suffix = 1u32;
        loop {
            let exists: bool = conn
                .prepare(&format!(
                    "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = '{name}'"
                ))
                .and_then(|mut stmt| stmt.query_row([], |row| row.get::<_, i64>(0)))
                .map(|count| count > 0)
                .unwrap_or(false);
            if !exists {
                break name;
            }
            suffix += 1;
            name = format!("{safe_name}_{suffix}");
        }
    };

    // Ingest the file
    conn.execute_batch(&format!(
        "CREATE TABLE \"{table_name}\" AS SELECT * FROM read_csv_auto('{path}')"
    ))
    .map_err(|e| format!("Failed to load file: {e}"))?;

    // Get column names
    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info('{table_name}')"))
            .map_err(|e| format!("Failed to get table info: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("Failed to read columns: {e}"))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Read rows (up to 10 000)
    let col_count = columns.len();
    let rows: Vec<Vec<String>> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT * FROM \"{table_name}\" LIMIT 10000"
            ))
            .map_err(|e| format!("Failed to query rows: {e}"))?;
        let mapped = stmt
            .query_map([], |row| {
                let mut vals = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let val: String = row
                        .get::<_, duckdb::types::Value>(i)
                        .map(|v| format_value(&v))
                        .unwrap_or_default();
                    vals.push(val);
                }
                Ok(vals)
            })
            .map_err(|e| format!("Failed to read rows: {e}"))?;
        mapped.filter_map(|r| r.ok()).collect()
    };

    Ok((table_name, TableData { columns, rows }))
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
