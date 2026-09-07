# Safety Model & Production Guards

Faultline executes real database migrations. To prevent accidental data corruption, safety is built into the core:

1. **Automatic Disposable Environments:**
   Faultline creates disposable ephemeral test databases (`faultline_test_<uuid>`) and drops them immediately after testing. It never alters your base database.
2. **Production Keywords Guard:**
   Faultline refuses to run if the database name contains production indicators (`prod`, `production`, `live`, `primary`, `master`, `customer`, `real`) unless `--allow-non-isolated` is explicitly provided.
3. **Cloud Host Protection:**
   Faultline refuses connections pointing to known public cloud database providers (`rds.amazonaws.com`, `cloudsql`, `database.azure.com`, `neon.tech`, `supabase.co`, etc.) without an explicit override.
4. **Clean Process Teardown:**
   Any background migration command process is spawned with `kill_on_drop(true)` to ensure child processes are terminated on timeout or interruption.
