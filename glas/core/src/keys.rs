//! Constants: JSON field names, marker strings and intent ids.
//!
//! Rule (from the project owner): no raw magic strings sprinkled through the
//! code — keys and markers live here. Same spirit as `types/keys.rs` in
//! opencode.

/// Intent JSON field: the intent (skill) id.
pub const FIELD_INTENT: &str = "intent";
/// Intent JSON field: the extracted parameters.
pub const FIELD_PARAMS: &str = "params";
/// Intent JSON field: the assistant's spoken/text reply.
pub const FIELD_REPLY: &str = "reply";

/// Intent id used when nothing matched.
pub const INTENT_UNKNOWN: &str = "unknown";

/// Builtin skill ids.
pub const SKILL_SET_TIMER: &str = "set_timer";
pub const SKILL_DELETE_TIMER: &str = "delete_timer";
pub const SKILL_DELETE_ALARM: &str = "delete_alarm";
pub const SKILL_SET_VOLUME: &str = "set_volume";
pub const SKILL_SEND_SMS: &str = "send_sms";

/// Parameter names shared across skills.
pub const PARAM_DURATION_SECONDS: &str = "duration_seconds";
pub const PARAM_TARGET: &str = "target";
pub const PARAM_LEVEL: &str = "level";
pub const PARAM_TO: &str = "to";
pub const PARAM_TEXT: &str = "text";
