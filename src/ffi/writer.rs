use crate::account::Account;
use akula::models::RlpAccount;
use anyhow::Result;
use ethers::types::{Address, H256};
use fastrlp::*;
use std::{
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
        let s = null_term(path.to_str().unwrap());
        let go_path = GoString::from(s.as_ref());

        let GoTuple { a: exit, b: db_ptr } = unsafe { MdbxOpen(go_path) };
        exit.ok_or_fmt("MdbxOpen")?;

        Ok(Self {
            path: path.to_path_buf(),
            db_ptr,
        })
    }

    pub fn close(mut self) -> Result<PathBuf> {
        unsafe { MdbxClose(self.db_ptr) }
        // consume without running drop()
        let path = mem::replace(&mut self.path, PathBuf::new());
        mem::forget(self);
        Ok(path)
    }

    pub fn put_head_header_hash(&mut self, mut hash: H256) -> Result<()> {
        let exit = unsafe { PutHeadHeaderHash(self.db_ptr, (&mut hash).into()) };
        exit.ok_or_fmt("PutAccount")?;
        Ok(())
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
        exit.ok_or_fmt("PutAccount")?;
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
        exit.ok_or_fmt("PutStorage")?;
        Ok(())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        unsafe { MdbxClose(self.db_ptr) }
    }
}
