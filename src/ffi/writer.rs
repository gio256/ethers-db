use crate::account::Account;
use akula::models::{self as ak_models, BlockHeader, BlockNumber, BodyForStorage, RlpAccount};
use anyhow::Result;
use bytes::BytesMut;
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

    pub fn put_header_number(&mut self, mut hash: H256, num: BlockNumber) -> Result<()> {
        let exit = unsafe { PutHeaderNumber(self.db_ptr, (&mut hash).into(), *num) };
        exit.ok_or_fmt("PutHeaderNumber")?;
        Ok(())
    }

    pub fn put_canonical_hash(&mut self, mut hash: H256, num: BlockNumber) -> Result<()> {
        let exit = unsafe { PutCanonicalHash(self.db_ptr, (&mut hash).into(), *num) };
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

    pub fn put_header(&mut self, header: BlockHeader) -> Result<()> {
        let mut buf = vec![];
        header.encode(&mut buf);

        let exit = unsafe { PutHeader(self.db_ptr, GoRlp((&mut buf[..]).into())) };
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

    pub fn put_transactions<T: IntoIterator<Item = ak_models::MessageWithSignature>>(
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

    pub fn put_senders<
        T: IntoIterator<Item = ak_models::Address>,
    >(
        &mut self,
        mut block_hash: H256,
        block_num: BlockNumber,
        senders: T,
    ) -> Result<()> {
        let mut bufs = vec![];
        for s in senders.into_iter() {
            let mut buf = BytesMut::new();
            s.encode(&mut buf);
            bufs.push(buf);
        }
        let mut go_slices = vec![];
        for buf in bufs.iter_mut() {
            go_slices.push(GoSlice::from(buf))
        }

        let exit = unsafe {
            PutSenders(
                self.db_ptr,
                (&mut block_hash).into(),
                *block_num,
                GoSlice::from(&mut go_slices[..]),
            )
        };
        exit.ok_or_fmt("PutSenders")?;

        Ok(())
    }

    pub fn put_body_for_storage(
        &mut self,
        mut hash: H256,
        num: ak_models::BlockNumber,
        body: BodyForStorage,
    ) -> Result<()> {
        let mut buf = vec![];
        body.encode(&mut buf);

        let exit = unsafe {
            PutBodyForStorage(
                self.db_ptr,
                GoU256::from(&mut hash),
                *num,
                GoRlp((&mut buf[..]).into()),
            )
        };
        exit.ok_or_fmt("PutBodyForStorage")?;
        Ok(())
    }

    pub fn put_tx_lookup_entries<T: IntoIterator<Item = ak_models::H256>>(
        &mut self,
        block_num: ak_models::BlockNumber,
        tx_hashes: T,
    ) -> Result<()> {
        let mut num = block_num.0.to_be_bytes();
        let mut tx_hashes = tx_hashes.into_iter().collect::<Vec<_>>();

        let mut bufs = vec![];
        for hash in tx_hashes.iter_mut() {
            bufs.push(GoU256::from(hash));
        }

        let exit = unsafe {
            PutTxLookupEntries(
                self.db_ptr,
                (&mut num[..]).into(),
                GoSlice::from(&mut bufs[..]),
            )
        };
        exit.ok_or_fmt("PutTxLookupEntries")?;
        Ok(())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        unsafe { MdbxClose(self.db_ptr) }
    }
}
