1. **Update `AppState` in `main.rs`**: Add a cache field to `AppState` to map file IDs to their full filenames. `cache: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, String>>>`.
2. **Initialize Cache in `main()`**: Before creating `AppState`, initialize the cache by synchronously scanning the `config.upload_dir` (`std::fs::read_dir`) and extracting the IDs (filenames without extensions). Store this cache in `AppState`.
3. **Update `handle_upload`**: After successfully saving a file, use the new file's ID to insert it into the cache: `state.cache.write().unwrap().insert(id.clone(), new_filename.clone());`.
4. **Update `handle_download`**: Instead of the O(N) async `fs::read_dir` loop to find the extension, use an O(1) cache lookup: `state.cache.read().unwrap().get(&id).map(|name| state.config.upload_dir.join(name))`.
5. **Update `run_cleanup`**: Modify the signature of `run_cleanup` to accept `&AppState` so it can remove expired files from the cache. Furthermore, follow memory guidelines by wrapping the bulk filesystem operations (like `fs::remove_file`) in `tokio::task::spawn_blocking` and using synchronous `std::fs` calls to minimize async executor overhead.
6. **Verify Source Changes**: Ensure `main.rs` compiles using `cargo check` and review the modified areas using `git diff` or `cat`.
7. **Run Benchmarks**: Run existing or newly created bash scripts to benchmark `handle_download` performance with a large number of dummy files to prove the optimization works.
8. **Pre-commit Steps**: Complete pre commit steps to ensure proper testing, verification, review, and reflection are done.
9. **Submit**: Create a PR with a description of the performance improvement and benchmarking results.
