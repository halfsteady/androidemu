fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    // SDL's Objective-C availability checks need Clang's runtime when targeting
    // older macOS versions. Rust links with -nodefaultlibs, omitting this archive.
    // Ask the selected compiler so both Xcode and Command Line Tools work.
    let output = cc::Build::new()
        .get_compiler()
        .to_command()
        .arg("-print-file-name=libclang_rt.osx.a")
        .output()
        .expect("locate the macOS Clang runtime");
    assert!(output.status.success(), "Clang runtime lookup failed");
    let archive = String::from_utf8(output.stdout).expect("Clang runtime path is UTF-8");
    let archive = std::path::Path::new(archive.trim());
    assert!(
        archive.is_file(),
        "Clang runtime archive not found: {}",
        archive.display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        archive.parent().unwrap().display()
    );
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
