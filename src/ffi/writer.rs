use crate::account::Account;
use akula::models::RlpAccount;
use anyhow::{bail, Result};
use ethers::types::{Address, H256};
use fastrlp::*;
use std::{
    ffi::CString,
    mem,
    path::{Path, PathBuf},
};

use super::interface::*;

pub struct Writer {
    path: PathBuf,
    db_ptr: GoPtr,
}
impl Writer {
    pub fn open<P: AsRef<Path>>(p: P) -> Result<Self> {
        // generate db path and open db
        let path = tempfile::Builder::new().tempdir_in(p)?.into_path();
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        let go_path = (&c_path).into();

        let GoTuple { a: exit, b: db_ptr } = unsafe { MdbxOpen(go_path) };

        if exit < 1 {
            bail!("MdbxOpen failed with exit code {}", exit)
        }
        Ok(Self {
            path: path.to_path_buf(),
            db_ptr,
        })
    }
    pub fn put_account(&mut self, mut who: Address, acct: Account) -> Result<()> {
        let rlp_acct: RlpAccount = acct.into();
        let mut buf = vec![];
        rlp_acct.encode(&mut buf);

        let exit = unsafe {
            PutAccount(
                self.db_ptr,
                (&mut who).into(),
                (&mut buf[..]).into(),
                acct.incarnation,
            )
        };
        if exit < 1 {
            bail!("MdbxOpen failed with exit code {}", exit)
        }
        Ok(())
    }

    pub fn put_storage(&mut self, mut who: Address, mut key: H256, mut val: H256) -> Result<()> {
        let exit = unsafe {
            PutStorage(
                self.db_ptr,
                (&mut who).into(),
                (&mut key).into(),
                (&mut val).into(),
            )
        };
        if exit < 1 {
            bail!("MdbxOpen failed with exit code {}", exit)
        }
        Ok(())
    }

    pub fn close(mut self) -> Result<PathBuf> {
        unsafe { MdbxClose(self.db_ptr) }
        // consume without running drop()
        let path = mem::replace(&mut self.path, PathBuf::new());
        mem::forget(self);
        Ok(path)
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        unsafe { MdbxClose(self.db_ptr) }
    }
}
