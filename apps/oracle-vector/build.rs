//! Windows only: embed the app icon and version info (VERSIONINFO) into `oracle-vector.exe`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `ORACLE_VECTOR_REQUIRE_WINRES=1` turns it into
//! an error (for release builds).

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/oracle-vector.ico");
    println!("cargo:rerun-if-env-changed=ORACLE_VECTOR_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/oracle-vector.ico")
        .set("ProductName", "Oracle Vector")
        .set("FileDescription", "Oracle Vector vector illustration editor")
        .set("CompanyName", "Oracle")
        .set("LegalCopyright", "Copyright (c) the Oracle Vector authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "oracle-vector.exe")
        .set("InternalName", "oracle-vector");
    if let Err(e) = res.compile() {
        if std::env::var_os("ORACLE_VECTOR_REQUIRE_WINRES").is_some() {
            return Err(format!("embedding Windows resources failed: {e}"));
        }
        println!("cargo:warning=oracle-vector.exe built without icon/version resources: {e}");
    }
    Ok(())
}
