use crate::account::Account;
use akula::models::RlpAccount;
use anyhow::Result;
use bytes::{BytesMut};
use ethers::types::{Address, Transaction, H256};
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
        let go_path = GoPath::from(s.as_ref());

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
        exit.ok_or_fmt("PutHeadHeaderHash")?;
        Ok(())
    }

    pub fn put_header_number(&mut self, mut hash: H256, num: u64) -> Result<()> {
        let exit = unsafe { PutHeaderNumber(self.db_ptr, (&mut hash).into(), num) };
        exit.ok_or_fmt("PutHeaderNumber")?;
        Ok(())
    }

    pub fn put_canonical_hash(&mut self, mut hash: H256, num: u64) -> Result<()> {
        let exit = unsafe { PutCanonicalHash(self.db_ptr, (&mut hash).into(), num) };
        exit.ok_or_fmt("PutCanonicalHash")?;
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
                GoRlp((&mut buf[..]).into()),
                acct.incarnation,
            )
        };
        exit.ok_or_fmt("PutAccount")?;
        Ok(())
    }

    //TODO: encoding is broken
    #[allow(unused)]
    pub fn put_raw_transactions<T: IntoIterator<Item = Transaction>>(
        &mut self,
        txs: T,
        base_id: u64,
    ) -> Result<()> {
        let mut txs = txs.into_iter().map(|tx| tx.rlp().0).collect::<Vec<_>>();

        let exit = unsafe { PutRawTransactions(self.db_ptr, (&mut txs[..]).into(), base_id) };
        exit.ok_or_fmt("PutRawTransactions")?;
        Ok(())
    }

    pub fn put_transactions<T: IntoIterator<Item = akula::models::MessageWithSignature>>(
        &mut self,
        txs: T,
        base_id: u64,
    ) -> Result<()> {
        let mut bufs = vec![];
        for tx in txs.into_iter() {
            let mut buf = BytesMut::new();
            tx.encode(&mut buf);
            bufs.push(buf);
        }
        let mut go_slices = vec![];
        for buf in bufs.iter_mut() {
            go_slices.push(GoSlice::from(buf))
        }

        let exit =
            unsafe { PutTransactions(self.db_ptr, GoSlice::from(&mut go_slices[..]), base_id) };
        exit.ok_or_fmt("PutTransactions")?;

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
