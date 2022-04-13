use std::{env, path::PathBuf, io::{self, Write}, process::Command};
const LINK_TEST_BIN: &str = "LINK_TEST_BIN";
const GO_PROJECT_DIR: &str = "dbfaker";
const GO_MAIN_FILE: &str = "main.go";

const GO_BIN_NAME: &str = "erigon";

fn main() {
    // re-run build script any time env var changes
    println!("cargo:rerun-if-env-changed={}", LINK_TEST_BIN);

    // Only link if env var is set for tests
    let is_test = env::var(LINK_TEST_BIN);
    if is_test.is_err() || is_test.unwrap().len() == 0 {
        println!(
            "cargo:warning=Set {} to link Erigon bindings if running tests",
            LINK_TEST_BIN
        );
        return;
    }

    let out_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cant get cargo OUT_DIR"));
    let go_dir = env::current_dir().expect("cant get cwd").join(GO_PROJECT_DIR);

    let out_file = out_dir.join(format!("lib{}.a", GO_BIN_NAME));
    dbg!(go_dir.clone());
    dbg!(out_file.clone());
    let output = Command::new("go")
        .arg("build")
        .arg("-buildmode=c-archive")
        .args(["-o", out_file.to_str().expect("bad out_file")])
        .arg(GO_MAIN_FILE)
        .current_dir(go_dir)
        .output().expect("failed to execute go build");

    println!("go build status: {}", output.status);
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
    assert!(output.status.success(), "failed to build go bindings");

    println!("cargo:rustc-link-lib=static={}", GO_BIN_NAME);
    println!("cargo:rustc-link-search=native={}", out_dir.to_str().unwrap());
}
