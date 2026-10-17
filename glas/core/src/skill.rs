//! Skills: the single `Skill` trait, manifests, parameters and results.
//!
//! See `ADR-0007` (declarative execution, one trait) and `ADR-0012` (the
//! optional `read` method for concrete clarifying questions).

use std::collections::BTreeMap;

use thiserror::Error;

use crate::system::System;

/// Concrete parameters extracted for a skill call.
pub type Params = BTreeMap<String, Value>;

/// A typed parameter value (kept small and typed on purpose — `serde_json::Value`
/// stays at the JSON boundary only).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl Value {
    /// Human-readable form for logs/prompts.
    pub fn as_display(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Bool(v) => v.to_string(),
            Value::Str(v) => v.clone(),
        }
    }
}

/// The declared type of a parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamKind {
    Int,
    Float,
    Bool,
    Str,
    Enum(Vec<String>),
}

impl ParamKind {
    /// Human-readable type name for the prompt catalogue.
    pub fn type_name(&self) -> &'static str {
        match self {
            ParamKind::Int => "int",
            ParamKind::Float => "float",
            ParamKind::Bool => "bool",
            ParamKind::Str => "string",
            ParamKind::Enum(_) => "enum",
        }
    }
}

/// One declared parameter of a skill.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamSpec {
    pub name: String,
    pub kind: ParamKind,
    pub required: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Optional human prompt used when clarifying ("На сколько минут?").
    pub question: Option<String>,
}

impl ParamSpec {
    pub fn required(name: &str, kind: ParamKind) -> Self {
        Self {
            name: name.to_string(),
            kind,
            required: true,
            min: None,
            max: None,
            question: None,
        }
    }

    pub fn optional(name: &str, kind: ParamKind) -> Self {
        Self {
            name: name.to_string(),
            kind,
            required: false,
            min: None,
            max: None,
            question: None,
        }
    }

    pub fn with_question(mut self, question: impl Into<String>) -> Self {
        self.question = Some(question.into());
        self
    }

    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
}

/// A skill's declared interface: id, description, examples, parameters, danger.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub examples: Vec<String>,
    pub params: Vec<ParamSpec>,
    pub danger: bool,
}

/// Data read from a skill (`read`), for answering or clarifying.
#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    Text(String),
    List(Vec<String>),
}

/// What a skill returns from `execute`.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// Did the action succeed?
    pub ok: bool,
    /// Short human-readable summary (goes to the UI and the context window).
    pub summary: String,
    /// The OS object id, when the OS gave one (a timer cookie, …). This is just
    /// text placed into the context line — not an object store (see `ADR-0012`).
    pub id: Option<String>,
}

impl Outcome {
    pub fn ok(summary: impl Into<String>) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            id: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Skill failures.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum SkillError {
    #[error("missing required parameter: {0}")]
    MissingParam(String),
    #[error("invalid value for {name}: {reason}")]
    InvalidValue { name: String, reason: String },
    #[error("skill does not support reading")]
    NoRead,
    #[error("execution failed: {0}")]
    Failed(String),
}

/// Execution context handed to a skill: the OS seam. Grows later (clock,
/// cancellation); a borrow keeps the core free of I/O.
pub struct Ctx<'a> {
    pub system: &'a dyn System,
}

/// The single skill interface the core works with (`ADR-0007`).
pub trait Skill {
    /// The declared interface.
    fn manifest(&self) -> &Manifest;

    /// Validate parameters. Default check is driven entirely by the manifest.
    fn validate(&self, params: &Params) -> Result<(), SkillError> {
        validate_params(self.manifest(), params)
    }

    /// Perform the action.
    fn execute(&self, params: &Params, ctx: &Ctx) -> Result<Outcome, SkillError>;

    /// Optional: read data (a list/value) for an answer or a concrete
    /// clarifying question. Defaults to "not supported" (`ADR-0012`).
    fn read(&self, _params: &Params, _ctx: &Ctx) -> Result<Data, SkillError> {
        Err(SkillError::NoRead)
    }
}

/// Declarative validation driven by a manifest: presence, type and range.
pub fn validate_params(manifest: &Manifest, params: &Params) -> Result<(), SkillError> {
    for spec in &manifest.params {
        match params.get(&spec.name) {
            None => {
                if spec.required {
                    return Err(SkillError::MissingParam(spec.name.clone()));
                }
            }
            Some(value) => {
                check_kind_and_range(spec, value)?;
            }
        }
    }
    Ok(())
}

fn check_kind_and_range(spec: &ParamSpec, value: &Value) -> Result<(), SkillError> {
    let num = match (&spec.kind, value) {
        (ParamKind::Int, Value::Int(v)) => Some(*v as f64),
        (ParamKind::Float, Value::Float(v)) => Some(*v),
        (ParamKind::Float, Value::Int(v)) => Some(*v as f64),
        (ParamKind::Bool, Value::Bool(_)) => None,
        (ParamKind::Str, Value::Str(_)) => None,
        (ParamKind::Enum(allowed), Value::Str(s)) => {
            if allowed.iter().any(|a| a == s) {
                None
            } else {
                return Err(SkillError::InvalidValue {
                    name: spec.name.clone(),
                    reason: format!("not one of {allowed:?}"),
                });
            }
        }
        _ => {
            return Err(SkillError::InvalidValue {
                name: spec.name.clone(),
                reason: "wrong type".to_string(),
            });
        }
    };

    if let Some(n) = num {
        if let Some(min) = spec.min {
            if n < min {
                return Err(SkillError::InvalidValue {
                    name: spec.name.clone(),
                    reason: format!("below minimum {min}"),
                });
            }
        }
        if let Some(max) = spec.max {
            if n > max {
                return Err(SkillError::InvalidValue {
                    name: spec.name.clone(),
                    reason: format!("above maximum {max}"),
                });
            }
        }
    }
    Ok(())
}
