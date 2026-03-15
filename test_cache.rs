use std::sync::{Arc, RwLock};
use std::collections::HashMap;

fn main() {
    let cache: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));
    cache.write().unwrap().insert("id".to_string(), ".txt".to_string());
    let ext = cache.read().unwrap().get("id").cloned();
    println!("{:?}", ext);
}
