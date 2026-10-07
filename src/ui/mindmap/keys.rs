//! Arrow-key movement through the map, which runs in rows: right goes
//! deeper along a row (path, topic, its steps), left comes back, and up/down
//! move between topic rows (to the step at the same depth).

use super::layout::{MapNode, NodeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// The node an arrow press moves to. With nothing selected, any arrow
/// selects the path itself.
pub fn neighbor(nodes: &[MapNode], from: Option<NodeKind>, direction: Direction) -> Option<NodeKind> {
    use Direction::*;
    use NodeKind::*;
    let Some(from) = from else {
        return Some(Root);
    };
    // The walk below is written along the tree (deeper = Down, siblings =
    // Left/Right); on screen the tree grows to the right.
    let direction = match direction {
        Right => Down,
        Left => Up,
        Up => Left,
        Down => Right,
    };
    let topics = nodes.iter().filter(|n| matches!(n.kind, Topic(_))).count();
    let subtopics =
        |t: usize| nodes.iter().filter(|n| matches!(n.kind, Subtopic(tt, _) if tt == t)).count();
    let sideways = |t: usize| -> Option<usize> {
        match direction {
            Left => t.checked_sub(1),
            Right => (t + 1 < topics).then_some(t + 1),
            _ => None,
        }
    };
    let to = match (from, direction) {
        (Root, Down) => Topic(0),
        (Topic(_), Up) => Root,
        (Topic(t), Down) => Subtopic(t, 0),
        (Topic(t), Left | Right) => Topic(sideways(t)?),
        (Subtopic(t, 0), Up) => Topic(t),
        (Subtopic(t, s), Up) => Subtopic(t, s - 1),
        (Subtopic(t, s), Down) => Subtopic(t, s + 1),
        (Subtopic(t, s), Left | Right) => {
            let next = sideways(t)?;
            match subtopics(next) {
                0 => Topic(next),
                count => Subtopic(next, s.min(count - 1)),
            }
        }
        _ => return None,
    };
    nodes.iter().any(|n| n.kind == to).then_some(to)
}

#[cfg(test)]
mod tests {
    use super::Direction::*;
    use super::*;
    use crate::model::sample_paths;
    use crate::ui::mindmap::layout::layout;

    #[test]
    fn arrows_walk_the_tree() {
        let nodes = layout(&sample_paths()[2]); // 3 topics, 2 subtopics each
        use NodeKind::*;
        assert_eq!(neighbor(&nodes, None, Right), Some(Root));
        assert_eq!(neighbor(&nodes, Some(Root), Right), Some(Topic(0)));
        assert_eq!(neighbor(&nodes, Some(Topic(0)), Right), Some(Subtopic(0, 0)));
        assert_eq!(neighbor(&nodes, Some(Subtopic(0, 0)), Right), Some(Subtopic(0, 1)));
        assert_eq!(neighbor(&nodes, Some(Subtopic(0, 1)), Right), None, "end of the row");
        assert_eq!(neighbor(&nodes, Some(Subtopic(0, 0)), Left), Some(Topic(0)));
        assert_eq!(neighbor(&nodes, Some(Topic(0)), Left), Some(Root));
        assert_eq!(neighbor(&nodes, Some(Subtopic(0, 1)), Down), Some(Subtopic(1, 1)));
        assert_eq!(neighbor(&nodes, Some(Topic(2)), Down), None, "last topic");
        assert_eq!(neighbor(&nodes, Some(Topic(0)), Up), None);
        assert_eq!(neighbor(&nodes, Some(Root), Up), None);
    }
}
