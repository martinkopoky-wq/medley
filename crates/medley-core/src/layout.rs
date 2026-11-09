//! The shareable layout format.
//!
//! Layouts are small JSON documents describing how panels are arranged. They
//! power the built-in presets, the community gallery (`community/layouts/` in
//! the repo), import/export, and the AI "describe your layout" feature.

use crate::store::{Accent, ThemeMode};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Library,
    Search,
    Collection,
    NowPlaying,
    Queue,
    AiStudio,
    Accounts,
    Appearance,
    Settings,
}

impl PanelKind {
    pub const ALL: [PanelKind; 9] = [
        Self::Library,
        Self::Search,
        Self::Collection,
        Self::NowPlaying,
        Self::Queue,
        Self::AiStudio,
        Self::Accounts,
        Self::Appearance,
        Self::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Search => "Search",
            Self::Collection => "Browse",
            Self::NowPlaying => "Now Playing",
            Self::Queue => "Up Next",
            Self::AiStudio => "AI Studio",
            Self::Accounts => "Accounts",
            Self::Appearance => "Appearance",
            Self::Settings => "Settings",
        }
    }

    /// One-line purpose, also fed to the layout AI.
    pub fn purpose(self) -> &'static str {
        match self {
            Self::Library => "sidebar with mixes, liked songs and playlists from every service",
            Self::Search => "search field and results across Spotify, SoundCloud and Apple Music",
            Self::Collection => "the open playlist, mix or album as a track list (main content area)",
            Self::NowPlaying => "large artwork, transport controls and AI liner notes for the current song",
            Self::Queue => "the upcoming songs, shuffle/repeat and 'keep the vibe' AI",
            Self::AiStudio => "turn a vibe description into a mix, or describe a layout",
            Self::Accounts => "connect and disconnect music services",
            Self::Appearance => "theme, accent color and the layout gallery",
            Self::Settings => "AI backend and other preferences",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Search => "search",
            Self::Collection => "collection",
            Self::NowPlaying => "now_playing",
            Self::Queue => "queue",
            Self::AiStudio => "ai_studio",
            Self::Accounts => "accounts",
            Self::Appearance => "appearance",
            Self::Settings => "settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDir {
    /// Side by side: `first` on the left.
    Horizontal,
    /// Stacked: `first` on top.
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LayoutNode {
    Split {
        split: SplitDir,
        /// Share of the space taken by `first`, 0.1–0.9.
        fraction: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
    Panels {
        panels: Vec<PanelKind>,
        #[serde(default)]
        active: usize,
    },
}

impl LayoutNode {
    pub fn leaf(panels: &[PanelKind]) -> Self {
        Self::Panels { panels: panels.to_vec(), active: 0 }
    }

    pub fn split(split: SplitDir, fraction: f32, first: LayoutNode, second: LayoutNode) -> Self {
        Self::Split { split, fraction, first: Box::new(first), second: Box::new(second) }
    }

    /// Panels of the left-/top-most leaf.
    pub fn first_leaf(&self) -> &[PanelKind] {
        match self {
            Self::Panels { panels, .. } => panels,
            Self::Split { first, .. } => first.first_leaf(),
        }
    }

    pub fn panels(&self) -> Vec<PanelKind> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<PanelKind>) {
        match self {
            Self::Panels { panels, .. } => out.extend(panels),
            Self::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }

    /// Remove duplicate panels and empty leaves. Returns `None` if nothing is left.
    fn cleaned(self, seen: &mut Vec<PanelKind>) -> Option<LayoutNode> {
        match self {
            Self::Panels { panels, active } => {
                let panels: Vec<_> = panels
                    .into_iter()
                    .filter(|p| {
                        let fresh = !seen.contains(p);
                        if fresh {
                            seen.push(*p);
                        }
                        fresh
                    })
                    .collect();
                if panels.is_empty() {
                    None
                } else {
                    let active = active.min(panels.len() - 1);
                    Some(Self::Panels { panels, active })
                }
            }
            Self::Split { split, fraction, first, second } => {
                let first = first.cleaned(seen);
                let second = second.cleaned(seen);
                match (first, second) {
                    (Some(a), Some(b)) => Some(Self::split(split, clamp_fraction(fraction), a, b)),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    (None, None) => None,
                }
            }
        }
    }

    /// Normalized rectangles `[x, y, w, h]` of every leaf, for previews.
    pub fn leaf_rects(&self) -> Vec<([f32; 4], Vec<PanelKind>)> {
        let mut out = Vec::new();
        self.rects_into([0.0, 0.0, 1.0, 1.0], &mut out);
        out
    }

    fn rects_into(&self, r: [f32; 4], out: &mut Vec<([f32; 4], Vec<PanelKind>)>) {
        match self {
            Self::Panels { panels, .. } => out.push((r, panels.clone())),
            Self::Split { split, fraction, first, second } => {
                let [x, y, w, h] = r;
                let (a, b) = match split {
                    SplitDir::Horizontal => {
                        let w1 = w * fraction;
                        ([x, y, w1, h], [x + w1, y, w - w1, h])
                    }
                    SplitDir::Vertical => {
                        let h1 = h * fraction;
                        ([x, y, w, h1], [x, y + h1, w, h - h1])
                    }
                };
                first.rects_into(a, out);
                second.rects_into(b, out);
            }
        }
    }
}

fn clamp_fraction(f: f32) -> f32 {
    if f.is_finite() { f.clamp(0.1, 0.9) } else { 0.5 }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSpec {
    #[serde(default = "default_format")]
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<Accent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeMode>,
    pub root: LayoutNode,
}

fn default_format() -> u32 {
    FORMAT_VERSION
}

impl LayoutSpec {
    pub fn new(name: impl Into<String>, root: LayoutNode) -> Self {
        Self {
            format: FORMAT_VERSION,
            name: name.into(),
            author: String::new(),
            description: String::new(),
            accent: None,
            theme: None,
            root,
        }
    }

    /// Parse and sanitize untrusted JSON (community files, clipboard, AI output).
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let spec: LayoutSpec = serde_json::from_str(json)?;
        anyhow::ensure!(
            spec.format <= FORMAT_VERSION,
            "layout uses format {}, this Medley understands {FORMAT_VERSION}",
            spec.format
        );
        Ok(spec.sanitized())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Dedupe panels, drop empty leaves, clamp fractions, make sure the main
    /// content panel exists, and bound string lengths.
    pub fn sanitized(mut self) -> Self {
        let mut seen = Vec::new();
        let mut root = self.root.cleaned(&mut seen).unwrap_or_else(|| LayoutNode::leaf(&[PanelKind::Collection]));
        if !seen.contains(&PanelKind::Collection) {
            root = LayoutNode::split(SplitDir::Horizontal, 0.35, root, LayoutNode::leaf(&[PanelKind::Collection]));
        }
        self.root = root;
        self.name = truncate(self.name.trim(), 48);
        if self.name.is_empty() {
            self.name = "Untitled".into();
        }
        self.author = truncate(self.author.trim(), 48);
        self.description = truncate(self.description.trim(), 200);
        self
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A simpler grid description that language models produce reliably; converted
/// into the tree form with [`ColumnsLayout::into_spec`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnsLayout {
    pub name: String,
    pub description: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub width: f32,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub height: f32,
    pub panels: Vec<PanelKind>,
}

impl ColumnsLayout {
    pub fn into_spec(self) -> LayoutSpec {
        let cols: Vec<(f32, LayoutNode)> = self
            .columns
            .into_iter()
            .filter(|c| !c.rows.is_empty())
            .map(|c| {
                let rows: Vec<(f32, LayoutNode)> =
                    c.rows.into_iter().map(|r| (r.height, LayoutNode::leaf(&r.panels))).collect();
                (c.width, chain(SplitDir::Vertical, rows))
            })
            .collect();
        let root = chain(SplitDir::Horizontal, cols);
        let mut spec = LayoutSpec::new(self.name, root);
        spec.description = self.description;
        spec.author = "AI".into();
        spec.sanitized()
    }
}

/// Fold weighted siblings into nested binary splits.
fn chain(dir: SplitDir, mut items: Vec<(f32, LayoutNode)>) -> LayoutNode {
    for item in &mut items {
        if !(item.0.is_finite() && item.0 > 0.0) {
            item.0 = 1.0;
        }
    }
    match items.len() {
        0 => LayoutNode::leaf(&[]),
        1 => items.pop().map(|i| i.1).unwrap_or_else(|| LayoutNode::leaf(&[])),
        _ => {
            let total: f32 = items.iter().map(|i| i.0).sum();
            let (w, first) = items.remove(0);
            LayoutNode::split(dir, w / total, first, chain(dir, items))
        }
    }
}

/// Presets bundled with the app. They are the same files the community gallery serves.
pub fn builtin_layouts() -> Vec<LayoutSpec> {
    [
        include_str!("../../../community/layouts/classic.json"),
        include_str!("../../../community/layouts/focus.json"),
        include_str!("../../../community/layouts/studio.json"),
        include_str!("../../../community/layouts/dj-booth.json"),
        include_str!("../../../community/layouts/compact.json"),
    ]
    .into_iter()
    .filter_map(|json| LayoutSpec::from_json(json).map_err(|e| tracing::error!("bad builtin layout: {e}")).ok())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        let layouts = builtin_layouts();
        assert_eq!(layouts.len(), 5);
        for l in layouts {
            assert!(l.root.panels().contains(&PanelKind::Collection), "{} lacks Browse", l.name);
        }
    }

    #[test]
    fn sanitize_dedupes_and_adds_content() {
        let json = r#"{"name":"x","root":{"split":"horizontal","fraction":4.0,
            "first":{"panels":["queue","queue"]},"second":{"panels":["queue"]}}}"#;
        let spec = LayoutSpec::from_json(json).unwrap();
        let panels = spec.root.panels();
        assert_eq!(panels.iter().filter(|p| **p == PanelKind::Queue).count(), 1);
        assert!(panels.contains(&PanelKind::Collection));
    }

    #[test]
    fn columns_convert() {
        let cols = ColumnsLayout {
            name: "Wide".into(),
            description: String::new(),
            columns: vec![
                Column { width: 1.0, rows: vec![Row { height: 1.0, panels: vec![PanelKind::Library] }] },
                Column {
                    width: 3.0,
                    rows: vec![
                        Row { height: 2.0, panels: vec![PanelKind::Collection] },
                        Row { height: 1.0, panels: vec![PanelKind::Queue] },
                    ],
                },
            ],
        };
        let spec = cols.into_spec();
        let rects = spec.root.leaf_rects();
        assert_eq!(rects.len(), 3);
        assert!((rects[0].0[2] - 0.25).abs() < 1e-4);
        assert!((rects[1].0[3] - 2.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn rejects_future_format() {
        assert!(LayoutSpec::from_json(r#"{"format":99,"name":"x","root":{"panels":["collection"]}}"#).is_err());
    }
}
