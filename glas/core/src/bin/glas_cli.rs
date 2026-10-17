//! `glas-cli` — a dev harness that drives the real core with a tiny rule-based
//! "brain" (the real LLM lives outside the core). Type an utterance and watch
//! the event stream; answer clarifications/confirmations the same way.
//!
//! Commands: `:yes` / `:no` (answer a confirmation), `:cancel`, `:q` to quit.

use std::cell::RefCell;
use std::io::{self, Write};

use glas_core::builtins::default_registry;
use glas_core::engine::{EngineError, TextGen};
use glas_core::memory::SimpleMemory;
use glas_core::{Command, Core, Event};

/// A keyword "brain" that mimics the LLM's JSON output for a few phrases.
/// Remembers the last `delete_*` intent so a bare answer continues it.
struct RuleGen {
    last_delete: RefCell<Option<String>>,
}

impl RuleGen {
    fn new() -> Self {
        Self {
            last_delete: RefCell::new(None),
        }
    }

    fn respond(&self, utterance: &str) -> String {
        let u = utterance.trim().to_lowercase();

        // Continue a pending delete with a bare answer like "8" or "08:00".
        let pending = self.last_delete.borrow().clone();
        if let Some(skill) = pending {
            if looks_like_answer(&u) {
                let target = normalize_time(&u);
                *self.last_delete.borrow_mut() = None;
                return json(&skill, &[("target", &target)], "Готово.");
            }
        }

        if u.contains("смс") || u.contains("sms") {
            return json(
                "send_sms",
                &[("to", "мама"), ("text", "привет")],
                "Отправить SMS маме: «привет»?",
            );
        }
        if u.contains("удали") || u.contains("убери") {
            if u.contains("будильник") {
                *self.last_delete.borrow_mut() = Some("delete_alarm".to_string());
                return json("delete_alarm", &[], "Какой будильник удалить?");
            }
            if u.contains("таймер") {
                *self.last_delete.borrow_mut() = Some("delete_timer".to_string());
                return json("delete_timer", &[], "Какой таймер удалить?");
            }
        }
        if u.contains("громкост") || u.contains("громче") || u.contains("тише") {
            let level = first_number(&u).unwrap_or(50).to_string();
            return json("set_volume", &[("level", &level)], "Меняю громкость.");
        }
        if u.contains("таймер") || u.contains("минут") || u.contains("секунд") {
            let minutes = first_number(&u).unwrap_or(5);
            let secs = (minutes * 60).to_string();
            return json(
                "set_timer",
                &[("duration_seconds", &secs)],
                "Ставлю таймер.",
            );
        }
        json("unknown", &[], "")
    }
}

impl TextGen for RuleGen {
    fn generate(&self, prompt: &str) -> Result<String, EngineError> {
        // The utterance is the last "User: ..." line of the prompt.
        let utterance = prompt
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix("User: "))
            .unwrap_or("")
            .to_string();
        Ok(self.respond(&utterance))
    }
}

fn json(intent: &str, params: &[(&str, &str)], reply: &str) -> String {
    let ps: Vec<String> = params
        .iter()
        .map(|(k, v)| {
            // Numbers go unquoted so typed params (Int) validate correctly.
            if v.parse::<i64>().is_ok() {
                format!("\"{k}\":{v}")
            } else {
                format!("\"{k}\":{v:?}")
            }
        })
        .collect();
    format!(
        "{{\"intent\":{intent:?},\"params\":{{{}}},\"reply\":{reply:?}}}",
        ps.join(",")
    )
}

fn first_number(s: &str) -> Option<i64> {
    let mut digits = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse().ok()
}

fn looks_like_answer(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit()) && s.len() <= 8
}

fn normalize_time(s: &str) -> String {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
    if t.contains(':') {
        t
    } else if t.len() <= 2 {
        format!("{t:0>2}:00")
    } else {
        t
    }
}

fn main() {
    let mut core = Core::new(
        default_registry(),
        Box::new(SimpleMemory::new(8)),
        Box::new(RuleGen::new()),
    );

    println!("glas-cli — type an utterance (:yes/:no/:cancel/:q).");
    let stdin = io::stdin();
    loop {
        print!("\n> ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let events = match line {
            ":q" => break,
            ":yes" | ":y" => core.handle(Command::Confirm { approved: true }),
            ":no" | ":n" => core.handle(Command::Confirm { approved: false }),
            ":cancel" => core.handle(Command::Cancel),
            _ => {
                core.handle(Command::CaptureStart);
                core.handle(Command::Transcribed {
                    text: line.to_string(),
                })
            }
        };
        for event in events {
            print_event(&event);
        }
    }
}

fn print_event(event: &Event) {
    match event {
        Event::StateChanged(s) => println!("  [{s}]"),
        Event::Transcribed { text } => println!("  you: {text}"),
        Event::SessionStarted {
            session_id,
            skill_id,
        } => println!("  (session #{session_id} opened: {skill_id})"),
        Event::SessionEnded { session_id } => println!("  (session #{session_id} closed)"),
        Event::NeedsConfirmation { reply, .. } => println!("  ASSISTANT (confirm?): {reply}"),
        Event::ActionResult { skill_id, summary, ok } => {
            println!("  action: {skill_id} (ok={ok}) {summary}")
        }
        Event::Reply { text } => println!("  ASSISTANT: {text}"),
        Event::UnknownUtterance { .. } => println!("  (unknown)"),
        Event::NoSuchSkill { intent } => println!("  (no such skill: {intent})"),
        Event::SkillDisabled { id } => println!("  (skill disabled: {id})"),
        Event::MemorySet { key, value } => println!("  (memory set: {key}={value})"),
        Event::MemoryForget { key } => println!("  (memory forget: {key})"),
        Event::Error { message } => println!("  ERROR: {message}"),
        Event::TurnDone => println!("  -- turn done --"),
    }
}
