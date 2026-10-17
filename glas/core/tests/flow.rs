//! End-to-end flow tests for the dialogue core — the scenarios from `ADR-0011`
//! (session / narrowing / confirmation) and `ADR-0012` (context references,
//! exit into a new skill, read-for-clarification).

use glas_core::builtins::default_registry;
use glas_core::context::Role;
use glas_core::engine::FakeGen;
use glas_core::memory::SimpleMemory;
use glas_core::registry::SkillRegistry;
use glas_core::system::FakeSystem;
use glas_core::{Command, Core, Event, TurnState};

/// A `System` that answers any OS call with a successful timer cookie.
fn fake_system() -> FakeSystem {
    let sys = FakeSystem::new();
    for cookie in 1..=16 {
        sys.push(Ok(format!("event added, cookie is {cookie}")));
    }
    sys
}

fn core_with(script: Vec<&str>) -> Core {
    let gen = FakeGen::new(script.into_iter().map(String::from).collect());
    Core::new(
        default_registry(),
        Box::new(SimpleMemory::new(8)),
        Box::new(gen),
        Box::new(fake_system()),
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
    assert_eq!(
        reply_text(&ev),
        Some("Не разобрал: «абракадабра». Сформулируй иначе.")
    );
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
    let mut c = Core::new(
        reg,
        Box::new(SimpleMemory::new(8)),
        Box::new(gen),
        Box::new(fake_system()),
    );

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

fn session_started(events: &[Event], skill: &str) -> Option<u64> {
    events.iter().find_map(|e| match e {
        Event::SessionStarted {
            session_id,
            skill_id,
        } if skill_id == skill => Some(*session_id),
        _ => None,
    })
}

#[test]
fn clarification_opens_and_closes_a_session() {
    let mut c = core_with(vec![
        r#"{"intent":"delete_alarm","params":{},"reply":""}"#,
        r#"{"intent":"delete_alarm","params":{"target":"08:00"},"reply":""}"#,
    ]);
    let ev = c.handle(Command::Transcribed {
        text: "удали будильник".into(),
    });
    let id = session_started(&ev, "delete_alarm").expect("clarifying session started");
    assert_eq!(c.session_id(), Some(id));

    let ev2 = c.handle(Command::Transcribed { text: "на 8".into() });
    assert!(ev2.contains(&Event::SessionEnded { session_id: id }));
    assert_eq!(c.session_id(), None);
}

#[test]
fn confirmation_session_ends_on_confirm() {
    let mut c = core_with(vec![
        r#"{"intent":"send_sms","params":{"to":"мама","text":"привет"},"reply":""}"#,
    ]);
    let ev = c.handle(Command::Transcribed {
        text: "отправь смс маме".into(),
    });
    let id = session_started(&ev, "send_sms").expect("confirming session started");

    let ev2 = c.handle(Command::Confirm { approved: true });
    assert!(ev2.contains(&Event::SessionEnded { session_id: id }));
    assert_eq!(c.session_id(), None);
}

#[test]
fn memory_set_and_forget_via_commands() {
    let mut c = core_with(vec![]);
    let ev = c.handle(Command::MemorySet {
        key: "volume_default".into(),
        value: "40".into(),
    });
    assert!(ev.contains(&Event::MemorySet {
        key: "volume_default".into(),
        value: "40".into(),
    }));
    assert!(c.memory().system_line().contains("volume_default=40"));

    let ev2 = c.handle(Command::MemoryForget {
        key: "volume_default".into(),
    });
    assert!(ev2.contains(&Event::MemoryForget {
        key: "volume_default".into(),
    }));
}

#[test]
fn memory_forget_unknown_key_reports_error() {
    let mut c = core_with(vec![]);
    let ev = c.handle(Command::MemoryForget { key: "nope".into() });
    assert!(ev.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn events_stream_in_turn_order() {
    // `handle_into` must surface stages live, in order, before the turn ends.
    let mut c = core_with(vec![
        r#"{"intent":"set_timer","params":{"duration_seconds":5},"reply":""}"#,
    ]);
    let mut stages = Vec::new();
    c.handle_into(Command::CaptureStart, &mut |e| {
        if let Event::StateChanged(s) = e {
            stages.push(s);
        }
    });
    c.handle_into(Command::CaptureStop, &mut |e| {
        if let Event::StateChanged(s) = e {
            stages.push(s);
        }
    });
    let mut reply: Option<String> = None;
    let mut turn_done_at = None;
    let mut last = 0;
    c.handle_into(
        Command::Transcribed {
            text: "таймер на 5 секунд".into(),
        },
        &mut |e| {
            last += 1;
            match e {
                Event::StateChanged(s) => {
                    assert!(turn_done_at.is_none(), "state after TurnDone");
                    stages.push(s);
                }
                Event::Reply { text } => reply = Some(text),
                Event::TurnDone => turn_done_at = Some(last),
                _ => {}
            }
        },
    );
    assert_eq!(
        stages,
        vec![
            TurnState::Recording,
            TurnState::Transcribing,
            TurnState::Thinking,
            TurnState::Executing,
            TurnState::Speaking,
            TurnState::Idle,
        ]
    );
    assert!(reply.is_some(), "a reply was emitted");
    assert_eq!(turn_done_at, Some(last), "TurnDone is the last event");
}

#[test]
fn no_speech_aborts_without_calling_the_model() {
    // The script is empty: if `NoSpeech` consulted the model, `FakeGen` would
    // come back `Exhausted` and the core would emit `Event::Error`.
    let mut c = core_with(vec![]);
    c.handle(Command::CaptureStart);
    c.handle(Command::CaptureStop);
    let ev = c.handle(Command::NoSpeech);

    assert!(!ev.iter().any(|e| matches!(e, Event::Error { .. })));
    assert!(reply_text(&ev).is_none(), "no reply on silence");
    assert!(ev.contains(&Event::TurnDone));
    assert_eq!(c.state(), TurnState::Idle);
}

#[test]
fn no_speech_keeps_a_pending_confirmation() {
    let mut c = core_with(vec![
        r#"{"intent":"send_sms","params":{"to":"мама","text":"привет"},"reply":""}"#,
    ]);
    c.handle(Command::Transcribed {
        text: "отправь смс маме".into(),
    });
    let id = c.session_id().expect("confirming session");
    assert_eq!(c.state(), TurnState::AwaitingConfirmation);

    // A silent press while the confirmation is pending: the mic reopens and
    // closes, but the session survives.
    c.handle(Command::CaptureStart);
    c.handle(Command::CaptureStop);
    let ev = c.handle(Command::NoSpeech);
    assert!(ev.contains(&Event::TurnDone));
    assert_eq!(c.state(), TurnState::AwaitingConfirmation);
    assert_eq!(c.session_id(), Some(id), "confirmation kept");

    let ev2 = c.handle(Command::Confirm { approved: true });
    assert!(has_action_result(&ev2, "send_sms"));
    assert_eq!(c.state(), TurnState::Idle);
}
