//! Builtin skills. The timer is real and talks to the OS through the `System`
//! seam (`timedclient-qt5`, see `ADR-0010`). The rest are still **fakes** that
//! do not touch the OS — they exist to exercise the flow (validation,
//! slot-filling, confirmation, `read`, ids) and are replaced through the same
//! `Skill` interface later.

use crate::keys;
use crate::skill::{
    Ctx, Data, Manifest, Outcome, ParamKind, ParamSpec, Params, Skill, SkillError, Value,
};

/// "поставь таймер на N минут" → creates a real `timed` event via
/// `timedclient-qt5` and returns its cookie as the id.
pub struct SetTimerSkill;

impl Skill for SetTimerSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: keys::SKILL_SET_TIMER.to_string(),
            name: "Таймер".to_string(),
            description: "Поставить таймер на N минут".to_string(),
            examples: vec![
                "поставь таймер на 5 минут".to_string(),
                "таймер 10 мин".to_string(),
            ],
            params: vec![ParamSpec::required(keys::PARAM_DURATION_SECONDS, ParamKind::Int)
                .with_range(1.0, 86400.0)
                .with_question("На сколько минут?")],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, ctx: &Ctx) -> Result<Outcome, SkillError> {
        let secs = duration_seconds(params)?;
        let event = format!(
            "APPLICATION={};TITLE={};ticker={secs}",
            keys::TIMED_APP,
            keys::TIMED_TITLE
        );
        let button = format!("TITLE={}", keys::TIMED_STOP_BUTTON);
        let out = ctx
            .system
            .run(
                keys::PROGRAM_TIMEDCLIENT,
                &["-b", button.as_str(), "-e", event.as_str()],
            )
            .map_err(SkillError::Failed)?;
        let cookie = parse_timed_cookie(&out)?;
        Ok(Outcome::ok(human_duration(secs)).with_id(format!("timer#{cookie}")))
    }
}

/// Pull the cookie out of `timedclient-qt5` stdout: `event added, cookie is 49`.
fn parse_timed_cookie(out: &str) -> Result<String, SkillError> {
    if out.contains("failed") {
        return Err(SkillError::Failed(out.trim().to_string()));
    }
    out.split("cookie is")
        .nth(1)
        .and_then(|rest| {
            let digits: String = rest.trim_start().chars().take_while(char::is_ascii_digit).collect();
            (!digits.is_empty()).then_some(digits)
        })
        .ok_or_else(|| SkillError::Failed(format!("no cookie in timed output: {}", out.trim())))
}

/// "300" → "таймер на 5 мин"; "45" → "таймер на 45 сек".
fn human_duration(secs: i64) -> String {
    if secs % 60 == 0 {
        format!("таймер на {} мин", secs / 60)
    } else {
        format!("таймер на {secs} сек")
    }
}

/// "удали таймер" → needs a target; can `read` the list to ask concretely.
pub struct DeleteTimerSkill;

impl Skill for DeleteTimerSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: keys::SKILL_DELETE_TIMER.to_string(),
            name: "Удалить таймер".to_string(),
            description: "Удалить таймер (по объекту/времени)".to_string(),
            examples: vec!["а теперь удали его".to_string(), "удали таймер".to_string()],
            params: vec![
                ParamSpec::required(keys::PARAM_TARGET, ParamKind::Str)
                    .with_question("Какой таймер?"),
            ],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        match params.get(keys::PARAM_TARGET) {
            Some(Value::Str(t)) if !t.is_empty() => {
                Ok(Outcome::ok(format!("таймер {t} удалён")))
            }
            _ => Err(SkillError::MissingParam(keys::PARAM_TARGET.to_string())),
        }
    }

    fn read(&self, _params: &Params, _ctx: &Ctx) -> Result<Data, SkillError> {
        Ok(Data::List(vec![
            "5 мин".to_string(),
            "10 мин".to_string(),
        ]))
    }
}

/// "удали будильник" → the ADR-0012 scenario (three alarms; read the list).
pub struct DeleteAlarmSkill;

impl Skill for DeleteAlarmSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: keys::SKILL_DELETE_ALARM.to_string(),
            name: "Удалить будильник".to_string(),
            description: "Удалить будильник".to_string(),
            examples: vec!["удали будильник".to_string()],
            params: vec![
                ParamSpec::required(keys::PARAM_TARGET, ParamKind::Str)
                    .with_question("Какой будильник?"),
            ],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        match params.get(keys::PARAM_TARGET) {
            Some(Value::Str(t)) if !t.is_empty() => {
                Ok(Outcome::ok(format!("будильник {t} удалён")))
            }
            _ => Err(SkillError::MissingParam(keys::PARAM_TARGET.to_string())),
        }
    }

    fn read(&self, _params: &Params, _ctx: &Ctx) -> Result<Data, SkillError> {
        Ok(Data::List(vec![
            "07:00".to_string(),
            "08:00".to_string(),
            "09:00".to_string(),
        ]))
    }
}

/// "поставь громкость на N" → a simple non-dangerous action.
pub struct SetVolumeSkill;

impl Skill for SetVolumeSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: keys::SKILL_SET_VOLUME.to_string(),
            name: "Громкость".to_string(),
            description: "Установить громкость 0–100".to_string(),
            examples: vec!["громкость 50".to_string()],
            params: vec![ParamSpec::required(keys::PARAM_LEVEL, ParamKind::Int)
                .with_range(0.0, 100.0)
                .with_question("На сколько процентов?")],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        match params.get(keys::PARAM_LEVEL) {
            Some(Value::Int(v)) => Ok(Outcome::ok(format!("громкость {v}"))),
            _ => Err(SkillError::MissingParam(keys::PARAM_LEVEL.to_string())),
        }
    }
}

/// A deliberately dangerous skill for confirmation tests: sends an SMS.
pub struct SendSmsSkill;

impl Skill for SendSmsSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: keys::SKILL_SEND_SMS.to_string(),
            name: "Отправить SMS".to_string(),
            description: "Отправить SMS (опасно)".to_string(),
            examples: vec!["отправь смс маме".to_string()],
            params: vec![
                ParamSpec::required(keys::PARAM_TO, ParamKind::Str),
                ParamSpec::required(keys::PARAM_TEXT, ParamKind::Str),
            ],
            danger: true,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        let to = match params.get(keys::PARAM_TO) {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(SkillError::MissingParam(keys::PARAM_TO.to_string())),
        };
        Ok(Outcome::ok(format!("SMS отправлено: {to}")))
    }
}

/// Build the default fake skill set used by tests and the dev CLI.
pub fn default_registry() -> crate::registry::SkillRegistry {
    let mut reg = crate::registry::SkillRegistry::new();
    reg.register(Box::new(SetTimerSkill));
    reg.register(Box::new(DeleteTimerSkill));
    reg.register(Box::new(DeleteAlarmSkill));
    reg.register(Box::new(SetVolumeSkill));
    reg.register(Box::new(SendSmsSkill));
    reg
}

/// Parse a duration given in seconds from a parameter map (Int only).
fn duration_seconds(params: &Params) -> Result<i64, SkillError> {
    match params.get(keys::PARAM_DURATION_SECONDS) {
        Some(Value::Int(v)) if *v > 0 => Ok(*v),
        _ => Err(SkillError::MissingParam(
            keys::PARAM_DURATION_SECONDS.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::FakeSystem;

    fn timer_params(secs: i64) -> Params {
        let mut p = Params::new();
        p.insert(keys::PARAM_DURATION_SECONDS.to_string(), Value::Int(secs));
        p
    }

    #[test]
    fn timer_runs_timedclient_and_returns_cookie_id() {
        let sys = FakeSystem::new();
        sys.push(Ok("event added, cookie is 49".to_string()));
        let ctx = Ctx { system: &sys };

        let out = SetTimerSkill
            .execute(&timer_params(600), &ctx)
            .expect("timer executes");
        assert_eq!(out.id.as_deref(), Some("timer#49"));
        assert_eq!(out.summary, "таймер на 10 мин");

        let call = &sys.calls()[0];
        assert_eq!(call.program, keys::PROGRAM_TIMEDCLIENT);
        assert_eq!(call.args[0], "-b");
        assert_eq!(call.args[2], "-e");
        assert!(call.args[3].contains("ticker=600"));
        assert!(call.args[3].contains("APPLICATION=glas"));
    }

    #[test]
    fn timer_failure_becomes_a_skill_error() {
        let sys = FakeSystem::new();
        sys.push(Err("timed unreachable".to_string()));
        let ctx = Ctx { system: &sys };
        let err = SetTimerSkill.execute(&timer_params(60), &ctx).unwrap_err();
        assert!(matches!(err, SkillError::Failed(_)), "got: {err}");
    }

    #[test]
    fn parse_cookie_rejects_an_error_dump() {
        let err = parse_timed_cookie("event adding failed, dumping event data:").unwrap_err();
        assert!(matches!(err, SkillError::Failed(_)), "got: {err}");
    }

    #[test]
    fn human_duration_switches_between_min_and_sec() {
        assert_eq!(human_duration(300), "таймер на 5 мин");
        assert_eq!(human_duration(45), "таймер на 45 сек");
    }
}
