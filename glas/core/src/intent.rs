//! The intent pipeline: build the prompt and parse the model's JSON call.

use serde_json::Value as Json;
use thiserror::Error;

use crate::keys;
use crate::memory::Memory;
use crate::registry::SkillRegistry;
use crate::skill::{Params, Value};

/// A resolved call: which skill, with which parameters, and the reply text.
#[derive(Debug, Clone, PartialEq)]
pub struct IntentCall {
    pub intent: String,
    pub params: Params,
    pub reply: String,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum IntentError {
    #[error("no JSON object in model output")]
    NoJson,
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("missing field: {0}")]
    MissingField(String),
    #[error("params must be an object")]
    ParamsNotObject,
}

/// Build the compact prompt for the current turn.
///
/// `[system]` = instruction + catalogue + output format; `[memory]` = the recent
/// window; `[user]` = the utterance. The catalogue comes from the registry, not
/// from memory (`ADR-0007`).
pub fn build_prompt(registry: &SkillRegistry, memory: &dyn Memory, utterance: &str) -> String {
    let mut p = String::new();
    p.push_str("Ты — Glas, голосовой ассистент на телефоне. Выбери один наиболее подходящий навык.\n");
    p.push_str("Верни ТОЛЬКО JSON: {\"intent\": <id>, \"params\": {..}, \"reply\": <текст>}.\n");
    p.push_str("Если ничего не подходит — intent \"unknown\".\n");
    p.push_str("Для запуска нужны ВСЕ обязательные параметры: если данных нет — задай уточняющий\n");
    p.push_str("вопрос в \"reply\" и оставь params пустыми.\n\n");
    p.push_str("Навыки:\n");
    p.push_str(&registry.catalog());
    p.push('\n');

    p.push_str("Примеры:\n");
    p.push_str("U: поставь таймер на 5 минут\n");
    p.push_str("{\"intent\":\"set_timer\",\"params\":{\"duration_seconds\":300},\"reply\":\"Таймер на 5 минут\"}\n");
    p.push_str("U: выключи свет\n");
    p.push_str("{\"intent\":\"unknown\",\"params\":{},\"reply\":\"\"}\n");

    let system_line = memory.system_line();
    if !system_line.is_empty() {
        p.push('\n');
        p.push_str(&system_line);
        p.push('\n');
    }

    let window: Vec<String> = memory
        .window()
        .into_iter()
        .map(|t| match t.role {
            crate::context::Role::User => format!("U: {}", t.text),
            crate::context::Role::Action => format!("A: {}", t.text),
        })
        .collect();
    if !window.is_empty() {
        p.push_str("\nRecent:\n");
        for line in window {
            p.push_str(&line);
            p.push('\n');
        }
    }

    p.push_str("\nUser: ");
    p.push_str(utterance);
    p.push('\n');
    p
}

/// Parse the model's raw output into an [`IntentCall`].
///
/// Tolerant to surrounding prose: the first `{` … last `}` is extracted.
pub fn parse(raw: &str) -> Result<IntentCall, IntentError> {
    let slice = extract_json(raw).ok_or(IntentError::NoJson)?;
    let value: Json =
        serde_json::from_str(slice).map_err(|e| IntentError::Json(e.to_string()))?;

    let intent = value
        .get(keys::FIELD_INTENT)
        .and_then(Json::as_str)
        .ok_or_else(|| IntentError::MissingField(keys::FIELD_INTENT.to_string()))?
        .to_string();

    let reply = value
        .get(keys::FIELD_REPLY)
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();

    let params = match value.get(keys::FIELD_PARAMS) {
        None | Some(Json::Null) => Params::new(),
        Some(Json::Object(map)) => {
            let mut out = Params::new();
            for (k, v) in map {
                out.insert(k.clone(), json_to_value(v));
            }
            out
        }
        Some(_) => return Err(IntentError::ParamsNotObject),
    };

    Ok(IntentCall {
        intent,
        params,
        reply,
    })
}

fn extract_json(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    Some(&raw[start..=end])
}

fn json_to_value(v: &Json) -> Value {
    match v {
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        Json::String(s) => Value::Str(s.clone()),
        other => Value::Str(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::default_registry;
    use crate::memory::SimpleMemory;

    #[test]
    fn prompt_is_russian_with_few_shots_and_catalog() {
        let reg = default_registry();
        let mem = SimpleMemory::default();
        let p = build_prompt(&reg, &mem, "поставь таймер");

        assert!(p.contains("Примеры:"), "few-shot section present");
        assert!(p.contains("U: поставь таймер на 5 минут"));
        assert!(p.contains("set_timer"), "catalogue present");
        assert!(p.trim_end().ends_with("User: поставь таймер"));
    }

    #[test]
    fn parses_json_surrounded_by_prose() {
        let call = parse(
            "конечно, вот: {\"intent\":\"set_timer\",\"params\":{\"duration_seconds\":300}} готово",
        )
        .unwrap();
        assert_eq!(call.intent, "set_timer");
        assert_eq!(call.params.get("duration_seconds"), Some(&Value::Int(300)));
    }
}
