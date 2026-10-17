//! The skill registry — the single source of truth for the prompt catalogue,
//! validation and execution (see `ADR-0007`).

use std::collections::BTreeSet;

use crate::skill::Skill;

/// Holds the registered skills and the set of disabled ids.
pub struct SkillRegistry {
    skills: Vec<Box<dyn Skill>>,
    disabled: BTreeSet<String>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            disabled: BTreeSet::new(),
        }
    }

    /// Register a skill. Duplicate ids are ignored (first wins) so a bad plugin
    /// cannot shadow a builtin.
    pub fn register(&mut self, skill: Box<dyn Skill>) {
        let id = skill.manifest().id.clone();
        if self.skills.iter().any(|s| s.manifest().id == id) {
            log::warn!("ignoring duplicate skill id: {id}");
            return;
        }
        self.skills.push(skill);
    }

    pub fn get(&self, id: &str) -> Option<&dyn Skill> {
        self.skills
            .iter()
            .find(|s| s.manifest().id == id)
            .map(|s| s.as_ref())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// A skill is usable only if registered and not disabled.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.contains(id) && !self.disabled.contains(id)
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        if on {
            self.disabled.remove(id);
        } else {
            self.disabled.insert(id.to_string());
        }
    }

    /// Ids of registered and enabled skills, in registration order.
    pub fn enabled_ids(&self) -> Vec<String> {
        self.skills
            .iter()
            .map(|s| s.manifest().id.clone())
            .filter(|id| !self.disabled.contains(id))
            .collect()
    }

    /// Compact catalogue for the prompt: one line per enabled skill.
    pub fn catalog(&self) -> String {
        let mut out = String::new();
        for skill in &self.skills {
            let m = skill.manifest();
            if self.disabled.contains(&m.id) {
                continue;
            }
            out.push_str(&format!("- {}: {}", m.id, m.description));
            if !m.params.is_empty() {
                let params: Vec<String> = m
                    .params
                    .iter()
                    .map(|p| {
                        let req = if p.required { "required" } else { "optional" };
                        format!("{}:{} {req}", p.name, p.kind.type_name())
                    })
                    .collect();
                out.push_str(&format!(" [params: {}]", params.join(", ")));
            }
            if !m.examples.is_empty() {
                out.push_str(&format!(" e.g. {}", m.examples.join("; ")));
            }
            out.push('\n');
        }
        out
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::builtins::default_registry;

    #[test]
    fn catalog_lists_param_types_and_requiredness() {
        let cat = default_registry().catalog();
        assert!(
            cat.contains("duration_seconds:int required"),
            "catalogue should name the type: {cat}"
        );
    }
}
