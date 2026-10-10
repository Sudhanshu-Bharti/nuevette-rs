// Embeds the app icon in the Windows exe. gpui's Windows backend loads icon
// resource 1 for the window and taskbar; Explorer uses it for the exe itself.
fn main() {
    println!("cargo:rerun-if-changed=assets/nuevette.rc");
    println!("cargo:rerun-if-changed=assets/nuevette.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/nuevette.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed the app icon");
    }
}
