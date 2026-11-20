//! Informational outline, never inferred from hyperlinks or used to teleport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub depth: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation {
    pub title: String,
    pub rows: Vec<Row>,
}
impl Navigation {
    pub fn load(root: &std::path::Path, ids: &[String]) -> crate::Result<Self> {
        let path = root.join("site-navigation.toml");
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::flat(ids)),
            Err(e) => Err(format!("navigation metadata: {e}")),
            Ok(_) => {
                let bytes = crate::content::read_local(root, "site-navigation.toml", 16384)?;
                Self::parse(
                    std::str::from_utf8(&bytes).map_err(|e| format!("navigation UTF-8: {e}"))?,
                    ids,
                )
            }
        }
    }
    pub fn parse(text: &str, ids: &[String]) -> crate::Result<Self> {
        use std::collections::{BTreeMap, BTreeSet};
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Declared {
            version: u32,
            name: String,
            pages: Vec<Node>,
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Node {
            id: String,
            label: String,
            parent: Option<String>,
        }
        if text.len() > 16384 || ids.is_empty() || ids.len() > 32 {
            return Err("navigation byte/page budget".into());
        }
        let d: Declared = toml::from_str(text).map_err(|e| format!("navigation TOML: {e}"))?;
        let label = |s: &str, max: usize| {
            !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
        };
        if d.version != 1 || !label(&d.name, 64) || d.pages.len() != ids.len() {
            return Err("navigation version/name/node count or missing page".into());
        }
        let mut nodes = BTreeMap::new();
        for n in &d.pages {
            if !ids.contains(&n.id)
                || !label(&n.label, 80)
                || nodes.insert(n.id.as_str(), n).is_some()
            {
                return Err(
                    "navigation unknown/duplicate page or label (one parent per page)".into(),
                );
            }
        }
        for n in &d.pages {
            let mut seen = BTreeSet::new();
            let mut at = n;
            let mut depth = 0;
            while let Some(p) = &at.parent {
                if !seen.insert(at.id.as_str()) {
                    return Err("navigation cycle/self parent".into());
                }
                at = nodes.get(p.as_str()).ok_or("navigation unknown parent")?;
                depth += 1;
                if depth > 8 {
                    return Err("navigation depth exceeds 8".into());
                }
            }
        }
        // At most 32*32 comparisons + 32*9 parent visits; source-order siblings.
        fn visit(parent: Option<&str>, depth: usize, nodes: &[Node], out: &mut Vec<Row>) {
            for n in nodes.iter().filter(|n| n.parent.as_deref() == parent) {
                out.push(Row {
                    id: n.id.clone(),
                    label: n.label.clone(),
                    depth,
                });
                visit(Some(&n.id), depth + 1, nodes, out);
            }
        }
        let mut rows = Vec::new();
        visit(None, 0, &d.pages, &mut rows);
        if rows.len() != ids.len() {
            return Err("navigation incomplete forest".into());
        }
        Ok(Self {
            title: format!("Host: {}", d.name),
            rows,
        })
    }
    pub fn flat(ids: &[String]) -> Self {
        Self {
            title: "Pages (flat)".into(),
            rows: ids
                .iter()
                .map(|id| Row {
                    id: id.clone(),
                    label: id.clone(),
                    depth: 0,
                })
                .collect(),
        }
    }
    pub fn current_index(&self, current: &str) -> Option<usize> {
        self.rows.iter().position(|r| r.id == current)
    }
}
