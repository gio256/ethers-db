use anyhow::{format_err, Result};
use once_cell::sync::Lazy;
use std::path::PathBuf;

pub mod ffi;
pub mod rand;

const TMP_DIR_ENV_LABEL: &str = "CHAINDATA_TMP_DIR";
const LINK_TEST_BIN: &str = "LINK_TEST_BIN";

pub(crate) static TMP_DIR: Lazy<PathBuf> = Lazy::new(|| tmp_dir().unwrap());

fn tmp_dir() -> Result<PathBuf> {
    let path = std::env::var(TMP_DIR_ENV_LABEL).map_err(|e| {
        if std::env::var(LINK_TEST_BIN).is_err() {
            format_err!("Err: {}\nExport {} to run the tests.", e, LINK_TEST_BIN)
        } else {
            format_err!(
                "Err: {}\nCan't get {}. This is likely a problem with the build script.",
                e,
                TMP_DIR_ENV_LABEL
            )
        }
    })?;
    Ok(PathBuf::from(path))
}
