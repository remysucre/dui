use crate::bridge::TableData;

/// Result of a SQL query.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    #[serde(default)]
    pub row_ids: Vec<i64>,
}

impl QueryResult {
    pub fn into_table_data(self) -> TableData {
        TableData {
            columns: self.columns,
            rows: self.rows,
            row_ids: self.row_ids,
        }
    }
}

/// Database abstraction so native and WASM builds share the same UI code.
pub trait Db {
    fn execute(&self, sql: &str) -> Result<(), String>;
    fn query(&self, sql: &str) -> Result<QueryResult, String>;
}

// ---------------------------------------------------------------------------
// Native implementation
// ---------------------------------------------------------------------------
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use duckdb::Connection;

    pub struct NativeDb {
        conn: Connection,
    }

    impl NativeDb {
        pub fn new() -> Self {
            Self {
                conn: Connection::open_in_memory().expect("Failed to open DuckDB"),
            }
        }
    }

    impl Db for NativeDb {
        fn execute(&self, sql: &str) -> Result<(), String> {
            self.conn
                .execute_batch(sql)
                .map_err(|e| format!("{e}"))
        }

        fn query(&self, sql: &str) -> Result<QueryResult, String> {
            let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
            let mut result = stmt.query([]).map_err(|e| e.to_string())?;

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
            let row_ids = Vec::new();
            while let Ok(Some(row)) = result.next() {
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

            Ok(QueryResult {
                columns,
                rows,
                row_ids,
            })
        }
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
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeDb;

// ---------------------------------------------------------------------------
// WASM implementation — calls JS globals via eframe's re-exported wasm-bindgen
// ---------------------------------------------------------------------------
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use eframe::wasm_bindgen::prelude::*;
    use eframe::wasm_bindgen::JsCast;

    /// Call a global JS function by name with a single string argument, returning a string.
    fn call_js(fn_name: &str, arg: &str) -> Result<String, String> {
        let window = eframe::web_sys::window().ok_or("no window")?;
        let func = js_sys::Reflect::get(&window, &JsValue::from_str(fn_name))
            .map_err(|_| format!("JS function {fn_name} not found"))?;
        let func: js_sys::Function = func
            .dyn_into()
            .map_err(|_| format!("{fn_name} is not a function"))?;
        let result = func
            .call1(&JsValue::NULL, &JsValue::from_str(arg))
            .map_err(|e| format!("JS call failed: {e:?}"))?;
        result
            .as_string()
            .ok_or_else(|| "JS function did not return a string".to_string())
    }

    pub struct WasmDb;

    impl WasmDb {
        pub fn new() -> Self {
            Self
        }
    }

    impl Db for WasmDb {
        fn execute(&self, sql: &str) -> Result<(), String> {
            let res = call_js("_ddb_exec", sql)?;
            if res.starts_with("ERROR:") {
                Err(res[6..].trim().to_string())
            } else {
                Ok(())
            }
        }

        fn query(&self, sql: &str) -> Result<QueryResult, String> {
            let json = call_js("_ddb_query", sql)?;
            if json.starts_with("ERROR:") {
                return Err(json[6..].trim().to_string());
            }
            serde_json::from_str(&json).map_err(|e| format!("JSON parse error: {e}"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmDb;
