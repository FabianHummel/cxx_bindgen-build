use build_print::info;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::str::from_utf8;

pub fn expand(
    features: &Vec<String>,
) -> String {
    let cargo = env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let mut cmd = Command::new(cargo);

    if let Ok(ref path) = env::var("OUT_DIR") {
        // https://github.com/mozilla/cbindgen/blob/b9b8f8878ac272935193c449066b88c0cb94ced2/src/bindgen/cargo/cargo_expand.rs#L81
        // "When cbindgen (cxx_bindgen-build) was started programatically from a build.rs file,
        // Cargo is running and locking the default target directory. In this case we need to use
        // another directory, else we would end up in a deadlock. If Cargo is running `OUT_DIR`
        // will be set, so we can use a directory relative to that."
        cmd.env("CARGO_TARGET_DIR", PathBuf::from(path).join("expanded"));
    }

    cmd.env("CXX_BINDGEN_RUNNING", env::var("CARGO_PKG_NAME").unwrap());

    cmd.args(["rustc", "--lib"]);

    if !features.is_empty() {
        cmd.arg("--features");
        cmd.arg(features.join(","));
    }

    cmd.args(["--", "-Zunpretty=expanded"]);

    info!("Running {:?}", cmd);

    let output = cmd.output().unwrap_or_else(|e| panic!("Failed to run cargo: {}", e));

    from_utf8(&output.stdout).expect("Failed to convert output to string").to_owned()
}