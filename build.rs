use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    process::Command,
};
const LINK_TEST_BIN: &str = "LINK_TEST_BIN";

// ** This dir gets rm -rf'd **
const DB_RM_DIR: &str = "build/chaindata";

const GO_PROJECT_DIR: &str = "dbfaker";
const GO_MAIN_FILE: &str = "main.go";
const GO_BIN_NAME: &str = "erigon";
const CHAINDATA_ENV_LABEL: &str = "ERIGON_CHAINDATA_DIR";

// Build and link the erigon go bindings. This is only for testing, so as a hack
// we only link if LINK_TEST_BIN is set.
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

    // sanity check
    let profile = env::var("PROFILE").expect("cant get build profile");
    if profile == "release" {
        panic!(
            "You probably don't want to link the test bindings in release mode. Unset {}",
            LINK_TEST_BIN
        );
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cant get cargo OUT_DIR"));
    let path = env::current_dir().expect("cant get cwd");
    let go_dir = path.join(GO_PROJECT_DIR);

    // build the go bindings
    let out_file = out_dir.join(format!("lib{}.a", GO_BIN_NAME));
    let output = Command::new("go")
        .arg("build")
        .arg("-buildmode=c-archive")
        .args(["-o", out_file.to_str().expect("bad out_file")])
        .arg(GO_MAIN_FILE)
        .current_dir(go_dir.clone())
        .output()
        .expect("failed to execute go build");

    println!("go build status: {}", output.status);
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
    assert!(output.status.success(), "failed to build go bindings");

    // clean db
    let rm_dir = path.join(DB_RM_DIR);
    if rm_dir.exists() {
        std::fs::remove_dir_all(rm_dir.clone()).expect("couldn't clean database");
    }
    // recreate chaindata dir
    std::fs::create_dir_all(&rm_dir).expect(&format!(
        "could not create dir: {}",
        rm_dir.to_string_lossy()
    ));

    // tell the tests where to find the db
    println!(
        "cargo:rustc-env={}={}",
        CHAINDATA_ENV_LABEL,
        rm_dir.to_str().unwrap()
    );

    // tell cargo to link the bindings
    println!("cargo:rustc-link-lib=static={}", GO_BIN_NAME);
    println!(
        "cargo:rustc-link-search=native={}",
        out_dir.to_str().unwrap()
    );

    // also re-run build script any time the go bindings or db change
    println!("cargo:rerun-if-changed={}", go_dir.to_str().unwrap());
    println!("cargo:rerun-if-changed={}", rm_dir.to_str().unwrap());
}
