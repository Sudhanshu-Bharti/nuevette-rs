//! Top-down tree layout, shaped for wide windows: the path at the top, topics
//! in a row beneath it, and each topic's subtopics stacked below it as an
//! indented list. Node sizes are fixed per kind so edges can be computed
//! before rendering.

use crate::model::LearningPath;

/// A world-space point or size, in canvas units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    pub fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    pub fn scale(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }

    pub fn tuple(self) -> (f32, f32) {
        (self.x, self.y)
    }
}

pub const ROOT_SIZE: Vec2 = Vec2::new(280., 156.);
pub const TOPIC_SIZE: Vec2 = Vec2::new(264., 148.);
pub const SUBTOPIC_SIZE: Vec2 = Vec2::new(248., 148.);

/// Between the path card and the topic column.
const ROOT_GAP: f32 = 96.;
/// Between one topic row and the next.
const ROW_GAP: f32 = 28.;
/// Between a topic and its first subtopic.
const TOPIC_TO_FIRST_SUBTOPIC: f32 = 48.;
/// Between steps along a row.
const SUBTOPIC_GAP: f32 = 28.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Root,
    Topic(usize),
    /// (topic index, subtopic index)
    Subtopic(usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapNode {
    pub kind: NodeKind,
    /// World-space top-left corner.
    pub pos: Vec2,
    pub size: Vec2,
    /// Index of the parent node, `None` for the root.
    pub parent: Option<usize>,
}

impl MapNode {
    pub fn center(&self) -> Vec2 {
        self.pos.add(self.size.scale(0.5))
    }
}

/// Rows, left to right: the path card on the left, centered on a column of
/// topics, and each topic's subtopics following it along its row in order.
pub fn layout(path: &LearningPath) -> Vec<MapNode> {
    let mut nodes = vec![MapNode {
        kind: NodeKind::Root,
        pos: Vec2::default(),
        size: ROOT_SIZE,
        parent: None,
    }];
    let topic_x = ROOT_SIZE.x + ROOT_GAP;
    let row_height = TOPIC_SIZE.y.max(SUBTOPIC_SIZE.y);

    for (ti, topic) in path.topics.iter().enumerate() {
        let y = ti as f32 * (row_height + ROW_GAP);
        let topic_ix = nodes.len();
        nodes.push(MapNode {
            kind: NodeKind::Topic(ti),
            pos: Vec2::new(topic_x, y),
            size: TOPIC_SIZE,
            parent: Some(0),
        });
        let mut x = topic_x + TOPIC_SIZE.x + TOPIC_TO_FIRST_SUBTOPIC;
        for si in 0..topic.subtopics.len() {
            nodes.push(MapNode {
                kind: NodeKind::Subtopic(ti, si),
                pos: Vec2::new(x, y),
                size: SUBTOPIC_SIZE,
                parent: Some(topic_ix),
            });
            x += SUBTOPIC_SIZE.x + SUBTOPIC_GAP;
        }
    }

    let rows = path.topics.len() as f32;
    let column_height = (rows * row_height + (rows - 1.).max(0.) * ROW_GAP).max(ROOT_SIZE.y);
    nodes[0].pos.y = (column_height - ROOT_SIZE.y) / 2.;
    nodes
}

/// Stable key for a node, used to save dragged positions.
pub fn node_key(kind: NodeKind) -> String {
    match kind {
        NodeKind::Root => "root".into(),
        NodeKind::Topic(t) => format!("t{t}"),
        NodeKind::Subtopic(t, s) => format!("s{t}.{s}"),
    }
}

/// The computed layout with any positions the learner saved applied on top.
pub fn layout_with_positions(path: &LearningPath) -> Vec<MapNode> {
    let mut nodes = layout(path);
    for node in &mut nodes {
        if let Some([x, y]) = path.positions.get(&node_key(node.kind)) {
            node.pos = Vec2::new(*x, *y);
        }
    }
    nodes
}

/// A smooth link from the right edge of `from` to the left edge of `to`: the
/// path into a topic, a topic into its first subtopic, or one step into the
/// next along a row.
pub fn edge_curve(from: &MapNode, to: &MapNode) -> [Vec2; 4] {
    let start = Vec2::new(from.pos.x + from.size.x, from.center().y);
    let end = Vec2::new(to.pos.x, to.center().y);
    // Never flatter than a short S, so links stay curves when cards are close
    // or dragged behind their parent.
    let bend = ((end.x - start.x) * 0.5).abs().max(16.);
    [start, Vec2::new(start.x + bend, start.y), Vec2::new(end.x - bend, end.y), end]
}

/// The smallest box holding every card: (top left, bottom right).
pub fn bounds(nodes: &[MapNode]) -> (Vec2, Vec2) {
    if nodes.is_empty() {
        return (Vec2::default(), Vec2::default());
    }
    let mut min = Vec2::new(f32::MAX, f32::MAX);
    let mut max = Vec2::new(f32::MIN, f32::MIN);
    for node in nodes {
        min = Vec2::new(min.x.min(node.pos.x), min.y.min(node.pos.y));
        let end = node.pos.add(node.size);
        max = Vec2::new(max.x.max(end.x), max.y.max(end.y));
    }
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::sample_paths;

    fn overlaps(a: &MapNode, b: &MapNode) -> bool {
        a.pos.x < b.pos.x + b.size.x
            && b.pos.x < a.pos.x + a.size.x
            && a.pos.y < b.pos.y + b.size.y
            && b.pos.y < a.pos.y + a.size.y
    }

    #[test]
    fn one_node_per_item_and_no_overlaps() {
        for path in sample_paths() {
            let nodes = layout(&path);
            assert_eq!(nodes.len(), 1 + path.topics.len() + path.subtopic_count());
            for (i, a) in nodes.iter().enumerate() {
                for b in &nodes[i + 1..] {
                    assert!(!overlaps(a, b), "{:?} overlaps {:?}", a.kind, b.kind);
                }
            }
        }
    }

    #[test]
    fn children_sit_right_of_parents_and_root_is_centered() {
        let nodes = layout(&sample_paths()[0]);
        for node in &nodes[1..] {
            let parent = &nodes[node.parent.unwrap()];
            assert!(node.pos.x > parent.pos.x + parent.size.x - 1.);
        }
        let (min, max) = bounds(&nodes);
        assert!((nodes[0].center().y - (min.y + max.y) / 2.).abs() < 1.);
    }

    #[test]
    fn wide_rather_than_tall_for_typical_paths() {
        let (min, max) = bounds(&layout(&sample_paths()[0]));
        let size = max.sub(min);
        assert!(size.x > size.y, "4 topics x 3 subtopics should be landscape: {size:?}");
    }

    #[test]
    fn edges_start_on_parent_and_end_on_child() {
        let nodes = layout(&sample_paths()[0]);
        for node in &nodes[1..] {
            let parent = &nodes[node.parent.unwrap()];
            let [from, _, _, to] = edge_curve(parent, node);
            assert!((from.x - (parent.pos.x + parent.size.x)).abs() < 1e-3);
            assert!((to.x - node.pos.x).abs() < 1e-3 && to.y >= node.pos.y - 1e-3);
        }
    }

    #[test]
    fn empty_path_still_has_a_root() {
        let mut path = sample_paths().remove(0);
        path.topics.clear();
        let nodes = layout(&path);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].pos, Vec2::default());
    }
}
