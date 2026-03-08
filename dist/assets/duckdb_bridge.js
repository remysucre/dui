// duckdb_bridge.js — async init for duckdb-wasm
//
// Uses conn.send() instead of conn.query() everywhere.
// conn.query() calls new arrow.Table(reader) internally which crashes
// on Safari with "this.source.peek" error. conn.send() returns a raw
// RecordBatchStreamReader that we drain manually via .next().

(async () => {
    try {
        console.log("duckdb_bridge: starting import...");
        var duckdb = await import("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/+esm");
        console.log("duckdb_bridge: import done, selecting bundle...");

        var bundles = await duckdb.selectBundle({
            mvp: {
                mainModule: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-mvp.wasm"),
                mainWorker: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-browser-mvp.worker.js"),
            },
            eh: {
                mainModule: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-eh.wasm"),
                mainWorker: new URL("https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.29.0/dist/duckdb-browser-eh.worker.js"),
            },
        });

        console.log("duckdb_bridge: bundle selected, creating worker...");

        var logger = new duckdb.ConsoleLogger();
        var db = new duckdb.AsyncDuckDB(logger);

        await db.instantiate(bundles.mainModule, bundles.mainWorker);
        console.log("duckdb_bridge: instantiated, connecting...");

        var conn = await db.connect();

        window._ddb_conn = conn;
        console.log("duckdb-wasm ready");

        // Drain a reader from conn.send(), collecting batches
        var drainReader = async function(reader) {
            var batches = [];
            while (true) {
                var step = await reader.next();
                if (!step || step.done) break;
                if (step.value) batches.push(step.value);
            }
            return batches;
        };

        // Convert schema + batches to our JSON format
        var batchesToJson = function(schema, batches) {
            var columns = [];
            if (schema && schema.fields) {
                for (var i = 0; i < schema.fields.length; i++) {
                    columns.push(schema.fields[i].name);
                }
            }
            var rows = [];
            for (var b = 0; b < batches.length; b++) {
                var batch = batches[b];
                for (var r = 0; r < batch.numRows; r++) {
                    var row = [];
                    for (var c = 0; c < columns.length; c++) {
                        var col = batch.getChildAt(c);
                        var val = col.get(r);
                        row.push(val == null ? "" : String(val));
                    }
                    rows.push(row);
                }
            }
            return { columns: columns, rows: rows, row_ids: [] };
        };

        // Run SQL and return JSON result (for SELECTs)
        var queryToJson = async function(sql) {
            var reader = await conn.send(sql);
            var schema = reader.schema;
            var batches = await drainReader(reader);
            return batchesToJson(schema, batches);
        };

        // Run DDL/DML — SQL executes during send(), no need to read results
        var execSql = async function(sql) {
            await conn.send(sql);
        };

        window._ddb_exec_async = async function(sql) {
            try {
                await execSql(sql);
                return "OK";
            } catch (e) {
                return "ERROR: " + (e.message || String(e));
            }
        };

        window._ddb_query_async = async function(sql) {
            try {
                return JSON.stringify(await queryToJson(sql));
            } catch (e) {
                return "ERROR: " + (e.message || String(e));
            }
        };

        window._ddb_batch_async = async function(jsonInput) {
            try {
                var input = JSON.parse(jsonInput);
                for (var i = 0; i < input.stmts.length; i++) {
                    await execSql(input.stmts[i]);
                }
                if (input.query) {
                    return JSON.stringify(await queryToJson(input.query));
                }
                return "OK";
            } catch (e) {
                return "ERROR: " + (e.message || String(e));
            }
        };

    } catch (e) {
        console.error("duckdb-wasm init failed:", e);
        window._ddb_init_error = e.message || String(e);
    }
})();
