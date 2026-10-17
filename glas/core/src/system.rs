//! OS primitives behind a seam (`ADR-0010`): skills reach the OS only through
//! this trait, never by shelling out themselves. The real implementation lives
//! outside the core (in the `glas-system` crate); the fake here keeps the whole
//! flow host-testable under `cargo test`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// One OS helper invocation (for assertions in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: String,
    pub args: Vec<String>,
}

/// Run an OS helper program. Returns its captured stdout on success.
pub trait System {
    fn run(&self, program: &str, args: &[&str]) -> Result<String, String>;
}

#[derive(Default)]
struct Inner {
    calls: Mutex<Vec<Call>>,
    replies: Mutex<VecDeque<Result<String, String>>>,
}

/// A scripted `System`: records calls and returns queued replies (default: empty
/// success). Cheap to clone — clones share the same log/queue.
#[derive(Clone, Default)]
pub struct FakeSystem {
    inner: Arc<Inner>,
}

impl FakeSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue the reply for the next `run` call.
    pub fn push(&self, reply: Result<String, String>) {
        self.inner.replies.lock().unwrap().push_back(reply);
    }

    /// Every `run` call, in order.
    pub fn calls(&self) -> Vec<Call> {
        self.inner.calls.lock().unwrap().clone()
    }
}

impl System for FakeSystem {
    fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
        self.inner.calls.lock().unwrap().push(Call {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        });
        self.inner
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(String::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_calls_and_returns_scripted_reply() {
        let sys = FakeSystem::new();
        sys.push(Ok("event added, cookie is 7".to_string()));
        let out = sys.run("timedclient-qt5", &["-e", "x=1"]).unwrap();
        assert_eq!(out, "event added, cookie is 7");
        assert_eq!(sys.calls().len(), 1);
        assert_eq!(sys.calls()[0].program, "timedclient-qt5");
        assert_eq!(sys.calls()[0].args, vec!["-e", "x=1"]);
    }
}
