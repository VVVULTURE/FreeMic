// Embeds the icon, version information and application manifest into the
// Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=packaging/assets/freemic.ico");
    println!("cargo:rerun-if-changed=packaging/windows/freemic.manifest");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/assets/freemic.ico");
        res.set_manifest_file("packaging/windows/freemic.manifest");
        res.set("ProductName", "FreeMic");
        res.set("FileDescription", "FreeMic - soundboard and virtual microphone");
        res.set("CompanyName", "FreeMic (github.com/VVVULTURE/FreeMic)");
        res.set("LegalCopyright", "Copyright (c) 2026 Vaden. MIT License.");
        res.set("OriginalFilename", "FreeMic.exe");
        res.set("InternalName", "FreeMic");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
