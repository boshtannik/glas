//! The turn state machine and dialogue sessions.
//!
//! This is the "worker + automaton" of `ADR-0008`, kept pure: `handle(command)`
//! advances the machine and returns the events to emit. Sessions
//! (confirmation / clarification) and the "exit into a new skill" rule live here
//! (`ADR-0011`, `ADR-0012`).

use crate::command::Command;
use crate::engine::TextGen;
use crate::event::Event;
use crate::intent::{self, IntentCall};
use crate::keys;
use crate::memory::Memory;
use crate::registry::SkillRegistry;
use crate::skill::{Ctx, Data, Manifest, Outcome, ParamKind, Params, SkillError, Value};
use crate::state::TurnState;

/// An open dialogue session (a multi-step turn).
#[derive(Debug)]
enum Session {
    /// No session — a fresh turn may start.
    None,
    /// A dangerous action is waiting for the user's yes/no.
    Confirming { skill_id: String, params: Params },
    /// A required parameter is missing; the skill holds the partially-filled call.
    Clarifying {
        skill_id: String,
        params: Params,
        missing: String,
    },
}

/// The dialogue core.
pub struct Core {
    state: TurnState,
    registry: SkillRegistry,
    memory: Box<dyn Memory>,
    gen: Box<dyn TextGen>,
    session: Session,
    session_id: Option<u64>,
    next_session_id: u64,
    ctx: Ctx,
}

impl Core {
    pub fn new(registry: SkillRegistry, memory: Box<dyn Memory>, gen: Box<dyn TextGen>) -> Self {
        Self {
            state: TurnState::Idle,
            registry,
            memory,
            gen,
            session: Session::None,
            session_id: None,
            next_session_id: 0,
            ctx: Ctx::default(),
        }
    }

    pub fn state(&self) -> TurnState {
        self.state
    }

    /// The id of the open session, if any (`ADR-0011`).
    pub fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    pub fn registry(&self) -> &SkillRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut SkillRegistry {
        &mut self.registry
    }

    pub fn memory(&self) -> &dyn Memory {
        self.memory.as_ref()
    }

    /// The single entry point: apply a command, get back the events to emit.
    pub fn handle(&mut self, cmd: Command) -> Vec<Event> {
        let mut ev = Vec::new();
        match cmd {
            Command::CaptureStart => {
                if self.state == TurnState::Idle || self.state == TurnState::AwaitingConfirmation {
                    self.set_state(TurnState::Recording, &mut ev);
                } else {
                    log::warn!("CaptureStart ignored in state {}", self.state);
                }
            }
            Command::CaptureStop => {
                if self.state == TurnState::Recording {
                    self.set_state(TurnState::Transcribing, &mut ev);
                } else {
                    log::warn!("CaptureStop ignored in state {}", self.state);
                }
            }
            Command::Cancel => {
                self.close_session(&mut ev);
                self.set_state(TurnState::Idle, &mut ev);
                ev.push(Event::TurnDone);
            }
            Command::Confirm { approved } => self.on_confirm(approved, &mut ev),
            Command::Transcribed { text } => self.on_transcribed(text, &mut ev),
            Command::MemorySet { key, value } => match self.memory.set(&key, &value) {
                Ok(()) => ev.push(Event::MemorySet { key, value }),
                Err(e) => ev.push(Event::Error {
                    message: e.to_string(),
                }),
            },
            Command::MemoryForget { key } => match self.memory.forget(&key) {
                Ok(()) => ev.push(Event::MemoryForget { key }),
                Err(e) => ev.push(Event::Error {
                    message: e.to_string(),
                }),
            },
            Command::Shutdown => {
                self.close_session(&mut ev);
                self.set_state(TurnState::Idle, &mut ev);
            }
        }
        ev
    }

    /// A new utterance is available (from STT, the CLI or a test).
    fn on_transcribed(&mut self, text: String, ev: &mut Vec<Event>) {
        ev.push(Event::Transcribed { text: text.clone() });

        // Voice confirmation: a yes/no while a confirmation is pending.
        if let Session::Confirming { .. } = self.session {
            if let Some(approved) = yes_no(&text) {
                self.on_confirm(approved, ev);
                return;
            }
        }

        // A pending clarification continues the same skill (`ADR-0012`): the
        // answer fills the missing parameter, unless it turns out to be a
        // different command (then we exit into a new skill).
        match std::mem::replace(&mut self.session, Session::None) {
            Session::Clarifying {
                skill_id,
                params,
                missing,
            } => {
                self.close_session(ev);
                self.run_clarified(&text, skill_id, params, missing, ev);
            }
            Session::Confirming { .. } => {
                // A confirmation that was not a yes/no → exit into a new skill.
                self.close_session(ev);
                self.run_intent(&text, ev);
            }
            Session::None => self.run_intent(&text, ev),
        }
    }

    /// Interpret `text` with the model without dispatching it. Returns `None`
    /// (after emitting `Event::Error`) if the model backend failed.
    fn interpret(&mut self, text: &str, ev: &mut Vec<Event>) -> Option<IntentCall> {
        self.set_state(TurnState::Thinking, ev);
        let prompt = intent::build_prompt(&self.registry, self.memory.as_ref(), text);
        self.memory.record_user(text);

        match self.gen.generate(&prompt) {
            Ok(raw) => Some(intent::parse(&raw).unwrap_or(IntentCall {
                intent: keys::INTENT_UNKNOWN.to_string(),
                params: Params::new(),
                reply: String::new(),
            })),
            Err(e) => {
                ev.push(Event::Error {
                    message: e.to_string(),
                });
                None
            }
        }
    }

    fn run_intent(&mut self, text: &str, ev: &mut Vec<Event>) {
        match self.interpret(text, ev) {
            Some(call) => self.dispatch(call, text, ev),
            None => self.finish(ev),
        }
    }

    /// Continue a skill after a clarifying question (`ADR-0012`).
    ///
    /// The stored skill and its partially-filled parameters are kept, so a bare
    /// answer ("на 8") can be filled in even by a weak model. A different command
    /// still exits into a new skill.
    fn run_clarified(
        &mut self,
        text: &str,
        skill_id: String,
        mut params: Params,
        missing: String,
        ev: &mut Vec<Event>,
    ) {
        let Some(call) = self.interpret(text, ev) else {
            self.finish(ev);
            return;
        };

        if call.intent == skill_id {
            // Same skill → merge the freshly extracted parameters.
            for (k, v) in call.params {
                params.insert(k, v);
            }
            self.dispatch(
                IntentCall {
                    intent: skill_id,
                    params,
                    reply: call.reply,
                },
                text,
                ev,
            );
        } else if call.intent.is_empty() || call.intent == keys::INTENT_UNKNOWN {
            // The model could not read the bare answer — fill the missing
            // parameter from the raw text directly.
            params.insert(missing.clone(), self.coerce(&skill_id, &missing, text));
            self.dispatch(
                IntentCall {
                    intent: skill_id,
                    params,
                    reply: String::new(),
                },
                text,
                ev,
            );
        } else {
            // A different command → exit into a new skill.
            self.dispatch(call, text, ev);
        }
    }

    /// Coerce a raw answer into the declared type of the missing parameter.
    fn coerce(&self, skill_id: &str, name: &str, text: &str) -> Value {
        let kind = self
            .manifest_of(skill_id)
            .and_then(|m| m.params.into_iter().find(|p| p.name == name))
            .map(|p| p.kind);
        let t = text.trim();
        match kind {
            Some(ParamKind::Int) => t
                .parse::<i64>()
                .map(Value::Int)
                .unwrap_or_else(|_| Value::Str(t.to_string())),
            Some(ParamKind::Float) => t
                .parse::<f64>()
                .map(Value::Float)
                .unwrap_or_else(|_| Value::Str(t.to_string())),
            Some(ParamKind::Bool) => Value::Bool(matches!(
                t.to_lowercase().as_str(),
                "да" | "yes" | "true" | "включи" | "вкл"
            )),
            _ => Value::Str(t.to_string()),
        }
    }

    fn dispatch(&mut self, call: IntentCall, text: &str, ev: &mut Vec<Event>) {
        if call.intent.is_empty() || call.intent == keys::INTENT_UNKNOWN {
            ev.push(Event::UnknownUtterance {
                text: text.to_string(),
            });
            ev.push(Event::Reply {
                text: "Не понял.".to_string(),
            });
            self.finish(ev);
            return;
        }

        if !self.registry.contains(&call.intent) {
            ev.push(Event::NoSuchSkill {
                intent: call.intent.clone(),
            });
            ev.push(Event::Reply {
                text: format!("Такого навыка нет: {}", call.intent),
            });
            self.finish(ev);
            return;
        }

        if !self.registry.is_enabled(&call.intent) {
            ev.push(Event::SkillDisabled {
                id: call.intent.clone(),
            });
            ev.push(Event::Reply {
                text: format!("Навык выключен: {}", call.intent),
            });
            self.finish(ev);
            return;
        }

        let manifest = self.manifest_of(&call.intent);
        match self.validate_call(&call.intent, &call.params) {
            Ok(()) => {
                if manifest.as_ref().map(|m| m.danger).unwrap_or(false) {
                    self.open_session(
                        Session::Confirming {
                            skill_id: call.intent.clone(),
                            params: call.params.clone(),
                        },
                        &call.intent,
                        ev,
                    );
                    self.set_state(TurnState::AwaitingConfirmation, ev);
                    let reply = if call.reply.is_empty() {
                        format!(
                            "Подтвердить «{}»?",
                            manifest.map(|m| m.name).unwrap_or_default()
                        )
                    } else {
                        call.reply.clone()
                    };
                    ev.push(Event::NeedsConfirmation {
                        skill_id: call.intent.clone(),
                        reply,
                    });
                } else {
                    self.execute(&call.intent, &call.params, ev);
                }
            }
            Err(SkillError::MissingParam(name)) => {
                self.ask_missing(&call.intent, call.params.clone(), &name, &call.reply, ev);
            }
            Err(other) => {
                ev.push(Event::Reply {
                    text: format!("Не получилось: {other}"),
                });
                self.finish(ev);
            }
        }
    }

    fn ask_missing(
        &mut self,
        skill_id: &str,
        params: Params,
        missing: &str,
        base_reply: &str,
        ev: &mut Vec<Event>,
    ) {
        let question = self.clarify_question(skill_id, &params, missing, base_reply);
        self.open_session(
            Session::Clarifying {
                skill_id: skill_id.to_string(),
                params,
                missing: missing.to_string(),
            },
            skill_id,
            ev,
        );
        // The mic is closed while we wait; the session stays open.
        self.set_state(TurnState::Idle, ev);
        ev.push(Event::Reply { text: question });
    }

    /// Build the clarifying question. If the skill can `read`, use the concrete
    /// list (`ADR-0012`); otherwise fall back to the parameter's declared
    /// question, the model's reply, or a generic question.
    fn clarify_question(
        &self,
        skill_id: &str,
        params: &Params,
        missing: &str,
        base_reply: &str,
    ) -> String {
        let param_q = self
            .manifest_of(skill_id)
            .and_then(|m| m.params.into_iter().find(|p| p.name == missing))
            .and_then(|p| p.question);

        if let Some(skill) = self.registry.get(skill_id) {
            if let Ok(data) = skill.read(params, &self.ctx) {
                match data {
                    Data::List(items) if !items.is_empty() => {
                        let label = param_q.unwrap_or_else(|| "Какой из них?".to_string());
                        return format!("{label} Варианты: {}.", items.join(", "));
                    }
                    Data::Text(t) => return t,
                    _ => {}
                }
            }
        }
        if let Some(q) = param_q {
            return q;
        }
        if !base_reply.is_empty() {
            return base_reply.to_string();
        }
        format!("Уточни, пожалуйста: {missing}.")
    }

    fn execute(&mut self, skill_id: &str, params: &Params, ev: &mut Vec<Event>) {
        self.set_state(TurnState::Executing, ev);

        let result = self
            .registry
            .get(skill_id)
            .map(|s| s.execute(params, &self.ctx));

        match result {
            Some(Ok(outcome)) => {
                self.memory
                    .record_action(&action_line(skill_id, params, &outcome));
                ev.push(Event::ActionResult {
                    skill_id: skill_id.to_string(),
                    summary: outcome.summary.clone(),
                    ok: outcome.ok,
                });
                self.set_state(TurnState::Speaking, ev);
                ev.push(Event::Reply {
                    text: outcome.summary,
                });
            }
            Some(Err(e)) => {
                log::warn!("skill {skill_id} failed: {e}");
                ev.push(Event::Error {
                    message: e.to_string(),
                });
                ev.push(Event::Reply {
                    text: format!("Не получилось: {e}"),
                });
            }
            None => {
                ev.push(Event::Error {
                    message: format!("unknown skill: {skill_id}"),
                });
            }
        }

        self.finish(ev);
    }

    fn on_confirm(&mut self, approved: bool, ev: &mut Vec<Event>) {
        let session = std::mem::replace(&mut self.session, Session::None);
        match session {
            Session::Confirming { skill_id, params } => {
                if approved {
                    self.execute(&skill_id, &params, ev);
                } else {
                    ev.push(Event::Reply {
                        text: "Отменено.".to_string(),
                    });
                    self.finish(ev);
                }
            }
            _ => {
                log::warn!("Confirm received with no pending confirmation");
                self.close_session(ev);
            }
        }
    }

    fn manifest_of(&self, id: &str) -> Option<Manifest> {
        self.registry.get(id).map(|s| s.manifest().clone())
    }

    fn validate_call(&self, id: &str, params: &Params) -> Result<(), SkillError> {
        match self.registry.get(id) {
            Some(s) => s.validate(params),
            None => Ok(()),
        }
    }

    fn set_state(&mut self, next: TurnState, ev: &mut Vec<Event>) {
        if self.state != next {
            self.state = next;
            ev.push(Event::StateChanged(next));
        }
    }

    /// Open a multi-step session and announce it (`ADR-0011`).
    fn open_session(&mut self, session: Session, skill_id: &str, ev: &mut Vec<Event>) {
        self.next_session_id += 1;
        let id = self.next_session_id;
        self.session = session;
        self.session_id = Some(id);
        ev.push(Event::SessionStarted {
            session_id: id,
            skill_id: skill_id.to_string(),
        });
    }

    /// Close the open session (if any) and announce it.
    fn close_session(&mut self, ev: &mut Vec<Event>) {
        if let Some(id) = self.session_id.take() {
            self.session = Session::None;
            ev.push(Event::SessionEnded { session_id: id });
        }
    }

    fn finish(&mut self, ev: &mut Vec<Event>) {
        self.close_session(ev);
        self.set_state(TurnState::Idle, ev);
        ev.push(Event::TurnDone);
    }
}

/// The compact context line for a finished action (params + the OS id, if any):
/// `set_timer {duration_seconds:300} → ok, id=timer#1` (see `Контекст-и-память`).
fn action_line(skill_id: &str, params: &Params, outcome: &Outcome) -> String {
    let status = if outcome.ok { "ok" } else { "fail" };
    let params_str = if params.is_empty() {
        String::new()
    } else {
        let pairs: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{k}:{}", v.as_display()))
            .collect();
        format!(" {{{}}}", pairs.join(", "))
    };
    match &outcome.id {
        Some(id) => format!("{skill_id}{params_str} → {status}, id={id}"),
        None => format!("{skill_id}{params_str} → {status}"),
    }
}

/// A tiny yes/no recogniser for voice confirmation (Russian first).
fn yes_no(text: &str) -> Option<bool> {
    let t = text.trim().to_lowercase();
    const YES: &[&str] = &["да", "ага", "угу", "верно", "подтверждаю", "конечно", "ок", "окей"];
    const NO: &[&str] = &["нет", "не", "отмена", "отменить", "не надо", "стоп"];
    if YES.contains(&t.as_str()) {
        Some(true)
    } else if NO.contains(&t.as_str()) {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yes_no_matches() {
        assert_eq!(yes_no("Да"), Some(true));
        assert_eq!(yes_no(" нет "), Some(false));
        assert_eq!(yes_no("удали его"), None);
    }
}
