use gpui::{App, KeyBinding, actions};

actions!(
    nuevette,
    [
        NewPath, Quit, OpenPalette, ZoomIn, ZoomOut, FitView, Deselect, ToggleDone, SelectUp,
        SelectDown, SelectLeft, SelectRight
    ]
);

pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    let map = Some("MindMap");
    cx.bind_keys([
        KeyBinding::new("ctrl-n", NewPath, None),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-k", OpenPalette, None),
        KeyBinding::new("ctrl-=", ZoomIn, map),
        KeyBinding::new("ctrl-+", ZoomIn, map),
        KeyBinding::new("ctrl--", ZoomOut, map),
        KeyBinding::new("ctrl-0", FitView, map),
        KeyBinding::new("escape", Deselect, map),
        KeyBinding::new("space", ToggleDone, map),
        KeyBinding::new("up", SelectUp, map),
        KeyBinding::new("down", SelectDown, map),
        KeyBinding::new("left", SelectLeft, map),
        KeyBinding::new("right", SelectRight, map),
    ]);
}
