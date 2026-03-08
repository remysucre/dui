// duckdb_bridge.js — synchronous JS glue for duckdb-wasm
//
// duckdb-wasm's "blocking" browser bundle gives us a synchronous API
// that we can call from Rust/wasm-bindgen without promises.
//
// We expose two global functions:
//   window._ddb_exec(sql)  → "OK" | "ERROR: …"
//   window._ddb_query(sql) → JSON string { columns, rows, row_ids } | "ERROR: …"

(function () {
    "use strict";

    let db = null;
    let conn = null;

    function ensureInit() {
        if (db) return;
        try {
            // The blocking bundle exposes duckdb on globalThis
            const DUCKDB_BUNDLES = {
                mvp: {
                    mainModule: "https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-mvp.wasm",
                    mainWorker: "https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-browser-mvp.worker.js",
                },
                eh: {
                    mainModule: "https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-eh.wasm",
                    mainWorker: "https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-browser-eh.worker.js",
                },
            };
            const bundle = duckdb.selectBundle(DUCKDB_BUNDLES);
            const logger = new duckdb.ConsoleLogger();
            db = new duckdb.DuckDBClient(logger, bundle);
            conn = db.connect();
        } catch (e) {
            console.error("duckdb-wasm init failed:", e);
        }
    }

    // --- Async version using the standard async API ---
    let asyncDb = null;
    let asyncConn = null;
    let initPromise = null;

    async function ensureAsyncInit() {
        if (asyncConn) return;
        if (initPromise) { await initPromise; return; }

        initPromise = (async () => {
            const DUCKDB_BUNDLES = await duckdb.selectBundle({
                mvp: {
                    mainModule: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-mvp.wasm"),
                    mainWorker: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-browser-mvp.worker.js"),
                },
                eh: {
                    mainModule: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-eh.wasm"),
                    mainWorker: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-browser-eh.worker.js"),
                },
            });

            const logger = new duckdb.ConsoleLogger();
            const worker = new Worker(DUCKDB_BUNDLES.mainWorker);
            asyncDb = new duckdb.AsyncDuckDB(logger, worker);
            await asyncDb.instantiate(DUCKDB_BUNDLES.mainModule);
            asyncConn = await asyncDb.connect();
        })();

        await initPromise;
    }

    window._ddb_exec = function (sql) {
        try {
            ensureInit();
            if (!conn) return "ERROR: duckdb not initialized";
            conn.query(sql);
            return "OK";
        } catch (e) {
            return "ERROR: " + (e.message || String(e));
        }
    };

    window._ddb_query = function (sql) {
        try {
            ensureInit();
            if (!conn) return "ERROR: duckdb not initialized";
            const result = conn.query(sql);

            // Arrow table → plain arrays
            const columns = result.schema.fields.map(f => f.name);
            const numRows = result.numRows;
            const rows = [];
            for (let r = 0; r < numRows; r++) {
                const row = [];
                for (let c = 0; c < columns.length; c++) {
                    const col = result.getChildAt(c);
                    const val = col.get(r);
                    row.push(val == null ? "" : String(val));
                }
                rows.push(row);
            }

            return JSON.stringify({ columns, rows, row_ids: [] });
        } catch (e) {
            return "ERROR: " + (e.message || String(e));
        }
    };

    // Async versions for use with wasm-bindgen-futures if needed
    window._ddb_exec_async = async function (sql) {
        try {
            await ensureAsyncInit();
            await asyncConn.query(sql);
            return "OK";
        } catch (e) {
            return "ERROR: " + (e.message || String(e));
        }
    };

    window._ddb_query_async = async function (sql) {
        try {
            await ensureAsyncInit();
            const result = await asyncConn.query(sql);

            const columns = result.schema.fields.map(f => f.name);
            const numRows = result.numRows;
            const rows = [];
            for (let r = 0; r < numRows; r++) {
                const row = [];
                for (let c = 0; c < columns.length; c++) {
                    const col = result.getChildAt(c);
                    const val = col.get(r);
                    row.push(val == null ? "" : String(val));
                }
                rows.push(row);
            }

            return JSON.stringify({ columns, rows, row_ids: [] });
        } catch (e) {
            return "ERROR: " + (e.message || String(e));
        }
    };

    // Hide the loading indicator once everything is ready
    const el = document.getElementById("loading");
    if (el) el.style.display = "none";
})();
