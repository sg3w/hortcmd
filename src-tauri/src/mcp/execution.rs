//! Per-request cancellation and bounded progress, shared with search/transfer
//! service primitives. Thread-local context stays on the blocking worker.
use serde_json::{json, Value};
use std::cell::RefCell;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub struct Execution {
    pub cancel: Arc<AtomicBool>,
    pub config: fn() -> super::McpConfig,
    pub progress_token: Option<Value>,
    pub output: Sender<Value>,
}

pub fn live_config() -> Option<super::McpConfig> {
    CURRENT.with(|c| c.borrow().as_ref().map(|context| (context.config)()))
}

thread_local! {
    static CURRENT: RefCell<Option<Execution>> = const { RefCell::new(None) };
}

pub fn run<T>(context: Execution, body: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT.with(|c| *c.borrow_mut() = None);
        }
    }
    CURRENT.with(|c| *c.borrow_mut() = Some(context));
    let _reset = Reset;
    body()
}

pub fn cancel_flag() -> Arc<AtomicBool> {
    CURRENT.with(|c| {
        c.borrow()
            .as_ref()
            .map(|c| c.cancel.clone())
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)))
    })
}

pub fn progress(done: u64, total: Option<u64>) {
    CURRENT.with(|c| {
        if let Some(context) = c.borrow().as_ref() {
            if let Some(token) = &context.progress_token {
                let mut params = json!({ "progressToken": token, "progress": done });
                if let Some(total) = total { params["total"] = json!(total); }
                let _ = context.output.try_send(json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": params }));
            }
        }
    });
}
