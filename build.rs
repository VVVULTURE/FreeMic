// Embeds the icon and version information into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=packaging/assets/freemic.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/assets/freemic.ico");
        res.set("ProductName", "FreeMic");
        res.set("FileDescription", "FreeMic - soundboard and virtual microphone");
        res.set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
