//! Eased camera moves. Selecting, fitting and focusing glide the viewport
//! instead of jumping; a new move interrupts the one in flight. Pointer pans
//! and wheel zoom stay direct.

use std::time::Duration;

use ely_gpui_component::canvas::Viewport;
use gpui::Context;

use super::MindMapView;

const DURATION_MS: u64 = 200;
const FRAME: Duration = Duration::from_millis(16);

/// `cubic-bezier(0.2, 0, 0, 1)` at time `t` in 0..=1.
pub fn ease(t: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.2f32, 0.0f32, 0.0f32, 1.0f32);
    let bezier = |a: f32, b: f32, s: f32| {
        let inv = 1. - s;
        3. * inv * inv * s * a + 3. * inv * s * s * b + s * s * s
    };
    // Solve x(s) = t for s by bisection; x is monotonic for these points.
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if bezier(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier(y1, y2, (lo + hi) / 2.)
}

fn lerp(from: Viewport, to: Viewport, k: f32) -> Viewport {
    // Interpolating the view center keeps zoom changes anchored visually.
    Viewport {
        x: from.x + (to.x - from.x) * k,
        y: from.y + (to.y - from.y) * k,
        zoom: from.zoom + (to.zoom - from.zoom) * k,
    }
}

impl MindMapView {
    /// Glides the viewport to `target`.
    pub(super) fn animate_to(&mut self, target: Viewport, cx: &mut Context<Self>) {
        let from = self.viewport;
        if from == target {
            return;
        }
        let steps = (DURATION_MS / FRAME.as_millis() as u64).max(1);
        self.camera = Some(cx.spawn(async move |this, cx| {
            for step in 1..=steps {
                cx.background_executor().timer(FRAME).await;
                let k = ease(step as f32 / steps as f32);
                let alive = this.update(cx, |map, cx| {
                    map.viewport = lerp(from, target, k);
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
    }

    /// Ends any glide, e.g. when the user grabs the canvas.
    pub(super) fn stop_camera(&mut self) {
        self.camera = None;
    }
}

#[cfg(test)]
mod tests {
    use super::ease;

    #[test]
    fn curve_starts_fast_and_settles() {
        assert!(ease(0.).abs() < 1e-3 && (ease(1.) - 1.).abs() < 1e-3);
        assert!(ease(0.25) > 0.5, "front-loaded: {}", ease(0.25));
        assert!(ease(0.5) < ease(0.75));
    }
}
