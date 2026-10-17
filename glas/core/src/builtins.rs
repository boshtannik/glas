//! Builtin skills. For now these are **fakes** that do not touch the OS — they
//! exist to exercise the flow (validation, slot-filling, confirmation, `read`,
//! ids). Real OS-backed implementations replace them later through the same
//! `Skill` interface (`ADR-0007`, `ADR-0010`).

use std::sync::atomic::{AtomicI64, Ordering};

use crate::skill::{
    Ctx, Data, Manifest, Outcome, ParamKind, ParamSpec, Params, Skill, SkillError, Value,
};

/// "поставь таймер на N минут" → registers a fake timer and returns its id.
pub struct SetTimerSkill {
    next_id: AtomicI64,
}

impl SetTimerSkill {
    pub fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1),
        }
    }
}

impl Default for SetTimerSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for SetTimerSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: "set_timer".to_string(),
            name: "Таймер".to_string(),
            description: "Поставить таймер на N минут".to_string(),
            examples: vec![
                "поставь таймер на 5 минут".to_string(),
                "таймер 10 мин".to_string(),
            ],
            params: vec![ParamSpec::required("duration_seconds", ParamKind::Int)
                .with_range(1.0, 86400.0)
                .with_question("На сколько минут?")],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        let secs = match params.get("duration_seconds") {
            Some(Value::Int(v)) => *v,
            _ => return Err(SkillError::MissingParam("duration_seconds".to_string())),
        };
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        Ok(Outcome::ok(format!("таймер на {} мин", secs / 60)).with_id(format!("timer#{id}")))
    }
}

/// "удали таймер" → needs a target; can `read` the list to ask concretely.
pub struct DeleteTimerSkill;

impl Skill for DeleteTimerSkill {
    fn manifest(&self) -> &Manifest {
        use std::sync::OnceLock;
        static M: OnceLock<Manifest> = OnceLock::new();
        M.get_or_init(|| Manifest {
            id: "delete_timer".to_string(),
            name: "Удалить таймер".to_string(),
            description: "Удалить таймер (по объекту/времени)".to_string(),
            examples: vec!["а теперь удали его".to_string(), "удали таймер".to_string()],
            params: vec![
                ParamSpec::required("target", ParamKind::Str).with_question("Какой таймер?"),
            ],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        match params.get("target") {
            Some(Value::Str(t)) if !t.is_empty() => {
                Ok(Outcome::ok(format!("таймер {t} удалён")))
            }
            _ => Err(SkillError::MissingParam("target".to_string())),
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
            id: "delete_alarm".to_string(),
            name: "Удалить будильник".to_string(),
            description: "Удалить будильник".to_string(),
            examples: vec!["удали будильник".to_string()],
            params: vec![
                ParamSpec::required("target", ParamKind::Str).with_question("Какой будильник?"),
            ],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        match params.get("target") {
            Some(Value::Str(t)) if !t.is_empty() => {
                Ok(Outcome::ok(format!("будильник {t} удалён")))
            }
            _ => Err(SkillError::MissingParam("target".to_string())),
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
            id: "set_volume".to_string(),
            name: "Громкость".to_string(),
            description: "Установить громкость 0–100".to_string(),
            examples: vec!["громкость 50".to_string()],
            params: vec![ParamSpec::required("level", ParamKind::Int)
                .with_range(0.0, 100.0)
                .with_question("На сколько процентов?")],
            danger: false,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        match params.get("level") {
            Some(Value::Int(v)) => Ok(Outcome::ok(format!("громкость {v}"))),
            _ => Err(SkillError::MissingParam("level".to_string())),
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
            id: "send_sms".to_string(),
            name: "Отправить SMS".to_string(),
            description: "Отправить SMS (опасно)".to_string(),
            examples: vec!["отправь смс маме".to_string()],
            params: vec![
                ParamSpec::required("to", ParamKind::Str),
                ParamSpec::required("text", ParamKind::Str),
            ],
            danger: true,
        })
    }

    fn execute(&self, params: &Params, _ctx: &Ctx) -> Result<Outcome, SkillError> {
        let to = match params.get("to") {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(SkillError::MissingParam("to".to_string())),
        };
        Ok(Outcome::ok(format!("SMS отправлено: {to}")))
    }
}

/// Build the default fake skill set used by tests and the dev CLI.
pub fn default_registry() -> crate::registry::SkillRegistry {
    let mut reg = crate::registry::SkillRegistry::new();
    reg.register(Box::new(SetTimerSkill::new()));
    reg.register(Box::new(DeleteTimerSkill));
    reg.register(Box::new(DeleteAlarmSkill));
    reg.register(Box::new(SetVolumeSkill));
    reg.register(Box::new(SendSmsSkill));
    reg
}
