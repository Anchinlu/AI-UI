use std::collections::HashMap;
use std::sync::Arc;
use tauri::async_runtime::{Mutex, JoinHandle};

pub struct AiTaskEntry {
    pub generation: u64,
    pub handle: JoinHandle<()>,
}

pub struct AiTaskManager {
    pub tasks: Arc<Mutex<HashMap<String, AiTaskEntry>>>,
    pub next_generation: Arc<Mutex<u64>>,
}

impl AiTaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            next_generation: Arc::new(Mutex::new(1)),
        }
    }
}
