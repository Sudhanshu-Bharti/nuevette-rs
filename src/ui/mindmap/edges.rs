//! Bezier edges between cards, painted under them in the canvas layer.
//! Ely's `InfiniteCanvas` draws the dot grid beneath.

use gpui::{Bounds, Hsla, PathBuilder, Pixels, Window, point, px};

use super::layout::Vec2;

pub struct Edge {
    /// Cubic bezier `[from, control_a, control_b, to]` in view pixels.
    pub curve: [Vec2; 4],
    pub highlighted: bool,
    /// Leads to finished work; drawn in the success color.
    pub done: bool,
}

pub struct EdgeLayer {
    pub edges: Vec<Edge>,
    pub color: Hsla,
    pub highlight: Hsla,
    pub done_color: Hsla,
    pub width: f32,
}

impl EdgeLayer {
    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let origin = bounds.origin;
        let at = |v: Vec2| point(origin.x + px(v.x), origin.y + px(v.y));
        // Highlighted edges last, so they draw over their neighbors.
        for highlighted in [false, true] {
            for edge in self.edges.iter().filter(|e| e.highlighted == highlighted) {
                let [from, control_a, control_b, to] = edge.curve;
                let width = if highlighted { self.width * 1.4 } else { self.width };
                let mut builder = PathBuilder::stroke(px(width));
                builder.move_to(at(from));
                builder.cubic_bezier_to(at(to), at(control_a), at(control_b));
                if let Ok(path) = builder.build() {
                    let color = match (highlighted, edge.done) {
                        (true, _) => self.highlight,
                        (false, true) => self.done_color,
                        (false, false) => self.color,
                    };
                    window.paint_path(path, color);
                }
            }
        }
    }
}
