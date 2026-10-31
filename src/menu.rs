//! CFD post-processing menu loaded from `agents/postprocess-menu.yaml`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Embedded copy so `flowplot menu` works even when cwd has no `agents/`.
const EMBEDDED_MENU_YAML: &str = include_str!("../agents/postprocess-menu.yaml");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostprocessMenu {
    pub version: u32,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub groups: Vec<MenuGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuGroup {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    /// `ready` dispatches; `stub` documents lehar + planned flags.
    pub status: String,
    #[serde(default)]
    pub lehar: LeharRef,
    pub flowplot: FlowplotRef,
    #[serde(default)]
    pub mcp: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeharRef {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowplotRef {
    pub command: String,
    pub template: String,
    #[serde(default)]
    pub required_args: Vec<String>,
    #[serde(default)]
    pub optional_flags: Vec<String>,
    #[serde(default)]
    pub planned_flags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl PostprocessMenu {
    pub fn load_default() -> Result<Self> {
        if let Ok(path) = env::var("FLOWPLOT_MENU") {
            return Self::load_path(Path::new(&path));
        }
        if let Some(path) = find_menu_file() {
            return Self::load_path(&path);
        }
        Self::from_yaml(EMBEDDED_MENU_YAML)
    }

    pub fn load_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|e| {
            Error::plot(format!(
                "failed to read menu {}: {e}",
                path.display()
            ))
        })?;
        Self::from_yaml(&text)
    }

    pub fn from_yaml(text: &str) -> Result<Self> {
        serde_yaml::from_str(text).map_err(|e| Error::plot(format!("invalid postprocess menu YAML: {e}")))
    }

    pub fn find_item(&self, id: &str) -> Option<(&MenuGroup, &MenuItem)> {
        for group in &self.groups {
            for item in &group.items {
                if item.id == id {
                    return Some((group, item));
                }
            }
        }
        None
    }

    pub fn filter_group(&self, group_id: Option<&str>) -> Vec<&MenuGroup> {
        match group_id {
            None => self.groups.iter().collect(),
            Some(gid) => self.groups.iter().filter(|g| g.id == gid).collect(),
        }
    }
}

fn find_menu_file() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("agents/postprocess-menu.yaml"),
        PathBuf::from("postprocess-menu.yaml"),
    ];
    for c in &candidates {
        if c.is_file() {
            return Some(c.clone());
        }
    }
    // Walk up from cwd a few levels (repo checkout).
    let mut dir = env::current_dir().ok()?;
    for _ in 0..6 {
        let candidate = dir.join("agents/postprocess-menu.yaml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Human-readable group listing.
pub fn format_menu_text(menu: &PostprocessMenu, group_id: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", menu.title));
    if !menu.description.trim().is_empty() {
        out.push_str(&format!("{}\n", menu.description.trim()));
    }
    out.push('\n');
    for group in menu.filter_group(group_id) {
        out.push_str(&format!("## {} ({})\n", group.title, group.id));
        if !group.summary.is_empty() {
            out.push_str(&format!("   {}\n", group.summary));
        }
        for item in &group.items {
            let mark = if item.status == "ready" { "*" } else { "-" };
            out.push_str(&format!(
                "  {mark} {id:<28} {title}  [{status}]\n",
                id = item.id,
                title = item.title,
                status = item.status
            ));
        }
        out.push('\n');
    }
    out.push_str("legend: * ready (menu run dispatches)  - stub (planned / needs more inputs)\n");
    out.push_str("usage:  flowplot menu run <id> -- <flowplot-args…>\n");
    out
}

pub fn format_item_text(group: &MenuGroup, item: &MenuItem) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "id: {}\ngroup: {} ({})\ntitle: {}\nstatus: {}\nsummary: {}\n",
        item.id, group.title, group.id, item.title, item.status, item.summary
    ));
    if let Some(agent) = &item.lehar.agent {
        out.push_str(&format!("lehar_agent: {agent}\n"));
    }
    if let Some(surface) = &item.lehar.surface {
        out.push_str(&format!("lehar_surface: {surface}\n"));
    }
    out.push_str(&format!(
        "flowplot_command: {}\ntemplate: {}\n",
        item.flowplot.command, item.flowplot.template
    ));
    if !item.flowplot.required_args.is_empty() {
        out.push_str(&format!(
            "required_args: {}\n",
            item.flowplot.required_args.join(", ")
        ));
    }
    if !item.flowplot.optional_flags.is_empty() {
        out.push_str(&format!(
            "optional_flags: {}\n",
            item.flowplot.optional_flags.join(" ")
        ));
    }
    if !item.flowplot.planned_flags.is_empty() {
        out.push_str(&format!(
            "planned_flags: {}\n",
            item.flowplot.planned_flags.join(" ")
        ));
    }
    if let Some(notes) = &item.flowplot.notes {
        out.push_str(&format!("notes: {}\n", notes.trim()));
    }
    if let Some(mcp) = &item.mcp {
        out.push_str(&format!("mcp: {mcp}\n"));
    }
    out
}

pub fn format_stub_run(item: &MenuItem) -> String {
    let mut out = String::new();
    out.push_str(&format!("stub: {}\n", item.title));
    if let Some(agent) = &item.lehar.agent {
        out.push_str(&format!("inspired_by: {agent}\n"));
    } else if let Some(surface) = &item.lehar.surface {
        out.push_str(&format!("inspired_by_surface: {surface}\n"));
    } else {
        out.push_str("stub — planned flags (see docs/postprocess-menu.md)\n");
    }
    out.push_str(&format!("flowplot: {}\n", item.flowplot.template));
    if !item.flowplot.planned_flags.is_empty() {
        out.push_str(&format!(
            "planned flags: {}\n",
            item.flowplot.planned_flags.join(" ")
        ));
    }
    if let Some(notes) = &item.flowplot.notes {
        out.push_str(&format!("notes: {}\n", notes.trim()));
    }
    if let Some(mcp) = &item.mcp {
        out.push_str(&format!("mcp: {mcp}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_menu_parses_and_has_ready_items() {
        let menu = PostprocessMenu::from_yaml(EMBEDDED_MENU_YAML).expect("yaml");
        // Categorized by CFD applicability (positivity, profiles, shocks, hypersonic, …).
        assert!(menu.groups.len() >= 10, "expected Agents categories, got {}", menu.groups.len());
        let group_ids: Vec<_> = menu.groups.iter().map(|g| g.id.as_str()).collect();
        for need in [
            "positivity-sanity",
            "profiles",
            "reference-values",
            "calculate-variables",
            "shocks",
            "hypersonic",
            "grid-mesh",
            "geometry-boundaries",
            "integration",
        ] {
            assert!(group_ids.contains(&need), "missing group {need}");
        }
        let ids: Vec<_> = menu
            .groups
            .iter()
            .flat_map(|g| g.items.iter().map(|i| i.id.as_str()))
            .collect();
        for need in [
            "load",
            "contour",
            "nondim-suggest",
            "pathlines",
            "contour-pathlines",
            "field-sanity",
            "positivity-check",
            "extract-profile",
            "mesh-inventory",
            "hypersonic-field-extrema",
            "shock-polar-field",
        ] {
            assert!(ids.contains(&need), "missing {need}");
        }
        let (_, load) = menu.find_item("load").unwrap();
        assert_eq!(load.status, "ready");
        let (_, sanity) = menu.find_item("field-sanity").unwrap();
        assert_eq!(sanity.status, "ready");
        let (_, bow) = menu.find_item("bow-shock-field").unwrap();
        assert_eq!(bow.status, "ready");
        assert_eq!(bow.lehar.agent.as_deref(), Some("bow_shock_validation"));
    }
}
