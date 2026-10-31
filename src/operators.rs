//! Operator stack stubs — Fieldplot Cut / Threshold / Level set.
//!
//! First slice wires the stack model and Cut→plane; Threshold / Level set
//! record parameters for later geometry apply.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::field::PlaneSpec;

/// One entry in the Fieldplot operator stack (VisIt-style operators).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operator {
    /// Structured cut / track plane (already used by mesh/contour).
    Cut { plane: String },
    /// Keep samples where `var` is inside `[min, max]` (stub apply).
    Threshold {
        variable: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// Iso-surface / level-set of `variable = value` (stub apply).
    LevelSet { variable: String, value: f64 },
}

impl Operator {
    pub fn label(&self) -> String {
        match self {
            Self::Cut { plane } => format!("Cut ({plane})"),
            Self::Threshold {
                variable,
                min,
                max,
            } => format!(
                "Threshold ({variable} ∈ [{}, {}])",
                min.map(|v| v.to_string()).unwrap_or_else(|| "-∞".into()),
                max.map(|v| v.to_string()).unwrap_or_else(|| "+∞".into())
            ),
            Self::LevelSet { variable, value } => format!("Level set ({variable} = {value})"),
        }
    }
}

/// Ordered operator stack (last entry is top / applied last).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OperatorStack {
    pub ops: Vec<Operator>,
}

impl OperatorStack {
    pub fn push(&mut self, op: Operator) {
        self.ops.push(op);
    }

    pub fn pop(&mut self) -> Option<Operator> {
        self.ops.pop()
    }

    pub fn clear(&mut self) {
        self.ops.clear();
    }

    /// Resolve the active Cut plane from the stack (last Cut wins).
    pub fn active_cut_plane(&self) -> Result<Option<PlaneSpec>> {
        let mut last = None;
        for op in &self.ops {
            if let Operator::Cut { plane } = op {
                last = Some(PlaneSpec::parse(plane)?);
            }
        }
        Ok(last)
    }

    /// Summarize stack for JSON / GUI (no geometry mutation yet beyond Cut).
    pub fn status_json(&self) -> String {
        let items: Vec<String> = self
            .ops
            .iter()
            .enumerate()
            .map(|(i, op)| {
                format!(
                    "{{\"index\":{i},\"kind\":{},\"label\":{}}}",
                    match op {
                        Operator::Cut { .. } => "\"cut\"",
                        Operator::Threshold { .. } => "\"threshold\"",
                        Operator::LevelSet { .. } => "\"level_set\"",
                    },
                    crate_json_string(&op.label())
                )
            })
            .collect();
        format!(
            "{{\"count\":{},\"ops\":[{}]}}",
            self.ops.len(),
            items.join(",")
        )
    }
}

fn crate_json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Parse a Threshold / Level set / Cut from CLI-ish flags.
pub fn parse_operator(
    kind: &str,
    plane: Option<&str>,
    variable: Option<&str>,
    min: Option<f64>,
    max: Option<f64>,
    value: Option<f64>,
) -> Result<Operator> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "cut" => {
            let plane = plane.unwrap_or("auto").to_string();
            let _ = PlaneSpec::parse(&plane)?;
            Ok(Operator::Cut { plane })
        }
        "threshold" => {
            let variable = variable
                .ok_or_else(|| Error::plot("threshold operator needs --var"))?
                .to_string();
            Ok(Operator::Threshold {
                variable,
                min,
                max,
            })
        }
        "level-set" | "level_set" | "levelset" | "iso" => {
            let variable = variable
                .ok_or_else(|| Error::plot("level set operator needs --var"))?
                .to_string();
            let value = value.ok_or_else(|| Error::plot("level set operator needs --value"))?;
            Ok(Operator::LevelSet { variable, value })
        }
        other => Err(Error::plot(format!(
            "unknown operator '{other}' (use cut, threshold, level-set)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_cut_and_threshold() {
        let mut stack = OperatorStack::default();
        stack.push(Operator::Cut {
            plane: "k=0".into(),
        });
        stack.push(Operator::Threshold {
            variable: "P".into(),
            min: Some(0.0),
            max: Some(1.0),
        });
        assert_eq!(stack.ops.len(), 2);
        let plane = stack.active_cut_plane().unwrap().unwrap();
        assert_eq!(plane, PlaneSpec::parse("k=0").unwrap());
        assert!(stack.status_json().contains("threshold"));
    }
}
