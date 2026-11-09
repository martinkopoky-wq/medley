//! Converting between the shareable [`LayoutSpec`] tree and egui_dock's state.

use egui_dock::{DockState, Node, NodeIndex, Tree};
use medley_core::layout::{LayoutNode, LayoutSpec, PanelKind, SplitDir};
use std::path::Path;

pub type Dock = DockState<PanelKind>;

/// Build a dock from a (sanitized) layout.
pub fn from_spec(spec: &LayoutSpec) -> Dock {
    let root = &spec.root;
    let mut dock = DockState::new(root.first_leaf().to_vec());
    build(dock.main_surface_mut(), NodeIndex::root(), root);
    dock
}

/// Invariant: the leaf at `at` already holds `node.first_leaf()`.
fn build(tree: &mut Tree<PanelKind>, at: NodeIndex, node: &LayoutNode) {
    match node {
        LayoutNode::Panels { active, .. } => {
            let _ = tree.set_active_tab(at, *active);
        }
        LayoutNode::Split { split, fraction, first, second } => {
            let fraction = if fraction.is_finite() { fraction.clamp(0.05, 0.95) } else { 0.5 };
            let new_tabs = second.first_leaf().to_vec();
            if new_tabs.is_empty() {
                build(tree, at, first);
                return;
            }
            let [a, b] = match split {
                SplitDir::Horizontal => tree.split_right(at, fraction, new_tabs),
                SplitDir::Vertical => tree.split_below(at, fraction, new_tabs),
            };
            build(tree, a, first);
            build(tree, b, second);
        }
    }
}

/// Describe the current arrangement as a shareable layout.
pub fn to_spec(dock: &Dock, name: &str) -> LayoutSpec {
    let tree = dock.main_surface();
    let mut root = walk(tree, NodeIndex::root()).unwrap_or_else(|| LayoutNode::leaf(&[PanelKind::Collection]));
    // Panels floating in windows (if any) join the main area as tabs.
    let floating: Vec<PanelKind> =
        dock.iter_surfaces().skip(1).flat_map(|s| s.iter_all_tabs().map(|(_, t)| *t).collect::<Vec<_>>()).collect();
    if !floating.is_empty() {
        root = LayoutNode::split(SplitDir::Horizontal, 0.75, root, LayoutNode::leaf(&floating));
    }
    LayoutSpec::new(name, root).sanitized()
}

fn walk(tree: &Tree<PanelKind>, at: NodeIndex) -> Option<LayoutNode> {
    if at.0 >= tree.len() {
        return None;
    }
    match &tree[at] {
        Node::Empty => None,
        Node::Leaf(leaf) => {
            (!leaf.tabs.is_empty()).then(|| LayoutNode::Panels { panels: leaf.tabs.clone(), active: leaf.active.0 })
        }
        Node::Horizontal(split) | Node::Vertical(split) => {
            let dir = if matches!(tree[at], Node::Horizontal(_)) { SplitDir::Horizontal } else { SplitDir::Vertical };
            match (walk(tree, at.left()), walk(tree, at.right())) {
                (Some(a), Some(b)) => Some(LayoutNode::split(dir, split.fraction, a, b)),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        }
    }
}

pub fn contains(dock: &Dock, panel: PanelKind) -> bool {
    dock.iter_all_tabs().any(|(_, t)| *t == panel)
}

/// Bring `panel` to the front, adding it to the focused area if it was closed.
pub fn focus(dock: &mut Dock, panel: PanelKind) {
    if let Some(path) = dock.find_tab(&panel) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
    } else {
        dock.push_to_focused_leaf(panel);
    }
}

pub fn load(path: &Path) -> Option<Dock> {
    let bytes = std::fs::read(path).ok()?;
    let dock: Dock = serde_json::from_slice(&bytes).ok()?;
    // Reject saved docks that lost the main content panel.
    contains(&dock, PanelKind::Collection).then_some(dock)
}

pub fn save(path: &Path, dock: &Dock) {
    match serde_json::to_vec(dock) {
        Ok(bytes) => {
            if let Err(e) = medley_core::store::write_atomic(path, &bytes, false) {
                tracing::warn!("saving layout: {e}");
            }
        }
        Err(e) => tracing::warn!("serializing layout: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medley_core::layout::builtin_layouts;

    #[test]
    fn builtins_roundtrip_through_the_dock() {
        for spec in builtin_layouts() {
            let dock = from_spec(&spec);
            let back = to_spec(&dock, &spec.name);
            let mut a = spec.root.panels();
            let mut b = back.root.panels();
            a.sort_by_key(|p| p.slug());
            b.sort_by_key(|p| p.slug());
            assert_eq!(a, b, "{}", spec.name);
            assert_eq!(spec.root.leaf_rects().len(), back.root.leaf_rects().len(), "{}", spec.name);
            for ((ra, _), (rb, _)) in spec.root.leaf_rects().iter().zip(back.root.leaf_rects().iter()) {
                for k in 0..4 {
                    assert!((ra[k] - rb[k]).abs() < 1e-3, "{}: {ra:?} vs {rb:?}", spec.name);
                }
            }
        }
    }

    #[test]
    fn focus_reopens_closed_panels() {
        let spec = builtin_layouts().remove(0);
        let mut dock = from_spec(&spec);
        let path = dock.find_tab(&PanelKind::Queue).unwrap();
        dock.remove_tab(path);
        assert!(!contains(&dock, PanelKind::Queue));
        focus(&mut dock, PanelKind::Queue);
        assert!(contains(&dock, PanelKind::Queue));
    }
}
