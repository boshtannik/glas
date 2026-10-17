//! End-to-end flow tests for the dialogue core — the scenarios from `ADR-0011`
//! (session / narrowing / confirmation) and `ADR-0012` (context references,
//! exit into a new skill, read-for-clarification).

use glas_core::builtins::default_registry;
use glas_core::context::Role;
use glas_core::engine::FakeGen;
use glas_core::memory::SimpleMemory;
use glas_core::registry::SkillRegistry;
use glas_core::{Command, Core, Event, TurnState};

fn core_with(script: Vec<&str>) -> Core {
    let gen = FakeGen::new(script.into_iter().map(String::from).collect());
    Core::new(
        default_registry(),
        Box::new(SimpleMemory::new(8)),
        Box::new(gen),
    )
}

fn has_action_result(events: &[Event], skill: &str) -> bool {
    events.iter().any(|e| {
        matches!(e, Event::ActionResult { skill_id, ok: true, .. } if skill_id == skill)
    })
}

fn reply_text(events: &[Event]) -> Option<&str> {
    events.iter().find_map(|e| match e {
        Event::Reply { text } => Some(text.as_str()),
        _ => None,
    })
}

#[test]
fn simple_action_runs_and_finishes() {
    let mut c = core_with(vec![
        r#"{"intent":"set_volume","params":{"level":50},"reply":""}"#,
    ]);
    let ev = c.handle(Command::Transcribed {
        text: "громкость 50".into(),
    });

    assert!(has_action_result(&ev, "set_volume"));
    assert!(ev.contains(&Event::TurnDone));
    assert_eq!(c.state(), TurnState::Idle);
}

#[test]
fn created_object_id_lands_in_context() {
    let mut c = core_with(vec![
        r#"{"intent":"set_timer","params":{"duration_seconds":300},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "таймер на 5 минут".into(),
    });

    let line = c
        .memory()
        .window()
        .into_iter()
        .find(|t| t.role == Role::Action && t.text.contains("set_timer"))
        .expect("action line recorded");
    assert!(line.text.contains("id=timer#1"), "got: {}", line.text);
}

#[test]
fn reference_to_previous_object_resolves_from_context() {
    // Turn 1 creates a timer (id=timer#1). Turn 2 refers to "его".
    let mut c = core_with(vec![
        r#"{"intent":"set_timer","params":{"duration_seconds":300},"reply":""}"#,
        r#"{"intent":"delete_timer","params":{"target":"timer#1"},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "таймер на 5 минут".into(),
    });
    let ev = c.handle(Command::Transcribed {
        text: "а теперь удали его".into(),
    });

    assert!(has_action_result(&ev, "delete_timer"));
    assert_eq!(c.state(), TurnState::Idle);
}

#[test]
fn missing_param_asks_with_a_concrete_list() {
    let mut c = core_with(vec![
        r#"{"intent":"delete_alarm","params":{},"reply":""}"#,
        r#"{"intent":"delete_alarm","params":{"target":"08:00"},"reply":""}"#,
    ]);

    let ev = c.handle(Command::Transcribed {
        text: "удали будильник".into(),
    });
    let q = reply_text(&ev).expect("a clarifying question");
    assert!(q.contains("07:00") && q.contains("08:00") && q.contains("09:00"), "got: {q}");

    // The answer continues the same skill.
    let ev2 = c.handle(Command::Transcribed {
        text: "на 8".into(),
    });
    assert!(has_action_result(&ev2, "delete_alarm"));
}

#[test]
fn clarification_bare_answer_filled_even_without_model_help() {
    // The model returns "unknown" for the bare answer "8", but the core still
    // fills the missing parameter directly (ADR-0012, "hold the context").
    let mut c = core_with(vec![
        r#"{"intent":"delete_alarm","params":{},"reply":""}"#,
        r#"{"intent":"unknown","params":{},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "удали будильник".into(),
    });
    let ev = c.handle(Command::Transcribed { text: "8".into() });
    assert!(has_action_result(&ev, "delete_alarm"));
}

#[test]
fn clarification_exits_into_a_different_skill() {
    let mut c = core_with(vec![
        r#"{"intent":"delete_alarm","params":{},"reply":""}"#,
        r#"{"intent":"set_volume","params":{"level":50},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "удали будильник".into(),
    });
    let ev = c.handle(Command::Transcribed {
        text: "громкость 50".into(),
    });
    assert!(has_action_result(&ev, "set_volume"));
    assert!(!has_action_result(&ev, "delete_alarm"));
}

#[test]
fn missing_param_without_read_uses_declared_question() {
    let mut c = core_with(vec![
        r#"{"intent":"set_timer","params":{},"reply":""}"#,
        r#"{"intent":"set_timer","params":{"duration_seconds":600},"reply":""}"#,
    ]);

    let ev = c.handle(Command::Transcribed {
        text: "поставь таймер".into(),
    });
    assert_eq!(reply_text(&ev), Some("На сколько минут?"));

    let ev2 = c.handle(Command::Transcribed {
        text: "10 минут".into(),
    });
    assert!(has_action_result(&ev2, "set_timer"));
}

#[test]
fn dangerous_action_needs_confirmation_then_runs() {
    let mut c = core_with(vec![
        r#"{"intent":"send_sms","params":{"to":"мама","text":"привет"},"reply":""}"#,
    ]);
    let ev = c.handle(Command::Transcribed {
        text: "отправь смс маме".into(),
    });
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::NeedsConfirmation { skill_id, .. } if skill_id == "send_sms")));
    assert_eq!(c.state(), TurnState::AwaitingConfirmation);

    let ev2 = c.handle(Command::Confirm { approved: true });
    assert!(has_action_result(&ev2, "send_sms"));
    assert_eq!(c.state(), TurnState::Idle);
}

#[test]
fn confirmation_by_voice_yes() {
    let mut c = core_with(vec![
        r#"{"intent":"send_sms","params":{"to":"мама","text":"привет"},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "отправь смс маме".into(),
    });
    let ev = c.handle(Command::Transcribed { text: "да".into() });
    assert!(has_action_result(&ev, "send_sms"));
}

#[test]
fn confirmation_rejected_cancels() {
    let mut c = core_with(vec![
        r#"{"intent":"send_sms","params":{"to":"мама","text":"привет"},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "отправь смс маме".into(),
    });
    let ev = c.handle(Command::Confirm { approved: false });
    assert_eq!(reply_text(&ev), Some("Отменено."));
    assert!(ev.contains(&Event::TurnDone));
    assert_eq!(c.state(), TurnState::Idle);
}

#[test]
fn non_answer_exits_into_a_new_skill() {
    let mut c = core_with(vec![
        r#"{"intent":"send_sms","params":{"to":"мама","text":"привет"},"reply":""}"#,
        r#"{"intent":"set_volume","params":{"level":50},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "отправь смс маме".into(),
    });
    assert_eq!(c.state(), TurnState::AwaitingConfirmation);

    // Not a yes/no → the pending confirmation is dropped and the new utterance
    // is resolved from scratch (ADR-0012: exit into a new skill).
    let ev = c.handle(Command::Transcribed {
        text: "громкость 50".into(),
    });
    assert!(has_action_result(&ev, "set_volume"));
}

#[test]
fn unknown_utterance_is_reported() {
    let mut c = core_with(vec![r#"{"intent":"unknown","params":{},"reply":""}"#]);
    let ev = c.handle(Command::Transcribed {
        text: "абракадабра".into(),
    });
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::UnknownUtterance { .. })));
    assert_eq!(reply_text(&ev), Some("Не понял."));
}

#[test]
fn unknown_skill_is_reported() {
    let mut c = core_with(vec![
        r#"{"intent":"fly_to_mars","params":{},"reply":""}"#,
    ]);
    let ev = c.handle(Command::Transcribed {
        text: "полети на марс".into(),
    });
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::NoSuchSkill { intent } if intent == "fly_to_mars")));
}

#[test]
fn disabled_skill_is_not_executed() {
    let mut reg: SkillRegistry = default_registry();
    reg.set_enabled("set_volume", false);
    let gen = FakeGen::new(vec![
        r#"{"intent":"set_volume","params":{"level":50},"reply":""}"#.to_string(),
    ]);
    let mut c = Core::new(reg, Box::new(SimpleMemory::new(8)), Box::new(gen));

    let ev = c.handle(Command::Transcribed {
        text: "громкость 50".into(),
    });
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::SkillDisabled { id } if id == "set_volume")));
    assert!(!has_action_result(&ev, "set_volume"));
}

#[test]
fn cancel_aborts_and_returns_to_idle() {
    let mut c = core_with(vec![]);
    c.handle(Command::CaptureStart);
    assert_eq!(c.state(), TurnState::Recording);

    let ev = c.handle(Command::Cancel);
    assert!(ev.contains(&Event::TurnDone));
    assert_eq!(c.state(), TurnState::Idle);
}
