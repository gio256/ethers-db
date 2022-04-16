use akula::kv::mdbx::MdbxEnvironment;
use anyhow::{format_err, Result};
use async_trait::async_trait;
use ethers::{
    core::types::{Address, Block, BlockId, NameOrAddress, TxHash, H256, U256, U64},
    providers::{FromErr, Middleware},
};
use mdbx::EnvironmentKind;
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;

use crate::db::*;
use crate::utils::{open_db, MsgCast};

#[derive(Debug)]
pub struct DbMiddleware<M, E: EnvironmentKind> {
    inner: M,
    db: Arc<MdbxEnvironment<E>>,
}

impl<M, E: EnvironmentKind> DbMiddleware<M, E> {
    /// Creates a new DbMiddleware instance given an inner provider and an
    /// mdbx environment with the chaindata.
    pub fn new(inner: M, db: Arc<MdbxEnvironment<E>>) -> Self {
        Self { inner, db }
    }

    /// Opens an mdbx environment using the path to the chaindata dir and creates
    /// a new DbMiddleware instance.
    pub fn open_new(inner: M, chaindata_dir: PathBuf) -> Result<Self> {
        let db = open_db(chaindata_dir)?;
        Ok(Self {
            inner,
            db: Arc::new(db),
        })
    }

    /// Begins a read-only MdbxTransaction and returns an Erigon db Reader
    pub fn reader(&self) -> Result<Reader<'_, mdbx::RO, E>> {
        Ok(Reader::new(self.db.begin()?))
    }
}

impl<M, E> DbMiddleware<M, E>
where
    M: Middleware,
    E: EnvironmentKind,
{
    async fn get_address<T: Into<NameOrAddress>>(
        &self,
        who: T,
    ) -> Result<Address, <Self as Middleware>::Error> {
        match who.into() {
            NameOrAddress::Name(name) => self.resolve_name(&name).await,
            NameOrAddress::Address(adr) => Ok(adr),
        }
    }
}

#[async_trait]
impl<M, E> Middleware for DbMiddleware<M, E>
where
    M: Middleware,
    E: EnvironmentKind,
{
    type Error = DbMiddlewareError<M>;
    type Provider = M::Provider;
    type Inner = M;

    fn inner(&self) -> &Self::Inner {
        &self.inner
    }

    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        let mut dbtx = self.reader()?;
        Ok(dbtx.read_head_block_number()?.0.into())
    }

    async fn get_balance<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        let mut dbtx = self.reader()?;
        Ok(dbtx.read_account_data(who)?.balance)
    }

    async fn get_transaction_count<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        let mut dbtx = self.reader()?;
        Ok(dbtx.read_account_data(who)?.nonce.into())
    }

    async fn get_transaction<T: Send + Sync + Into<TxHash>>(
        &self,
        transaction_hash: T,
    ) -> Result<Option<ethers::types::Transaction>, Self::Error> {
        let hash = transaction_hash.into();

        let mut dbtx = self.reader()?;
        let block_num = dbtx.read_transaction_block_number(hash)?;
        let header_key = dbtx.get_header_key(block_num.0)?;
        let body = dbtx.read_body_for_storage(header_key)?;

        let (msg, idx) = dbtx
            .read_transactions(body.base_tx_id.0, body.tx_amount)?
            .zip(0..)
            .find(|(msg, _i)| msg.hash() == hash)
            .unwrap();

        Ok(Some(MsgCast(&msg).cast(block_num, header_key.1, idx)))
    }

    async fn get_storage_at<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        location: H256,
        block: Option<BlockId>,
    ) -> Result<H256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;

        let mut dbtx = self.reader()?;
        let acct = dbtx.read_account_data(who)?;
        dbtx.read_account_storage(who, acct.incarnation, location)
            .map_err(From::from)
    }

    async fn get_uncle_count<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<U256, Self::Error> {
        let mut dbtx = self.reader()?;
        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let body = dbtx.read_body_for_storage(header_key)?;
        Ok(body.uncles.len().into())
    }

    async fn get_uncle<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
        idx: U64,
    ) -> Result<Option<Block<H256>>, Self::Error> {
        let mut dbtx = self.reader()?;
        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let body = dbtx.read_body_for_storage(header_key)?;
        let idx = idx.as_usize();
        if idx < body.uncles.len() {
            self.get_block(body.uncles[idx].number.0).await
        } else {
            Ok(None)
        }
    }

    // https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/bin/akula.rs#L266
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let mut dbtx = self.reader()?;

        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let (block_num, block_hash) = header_key;

        let header = dbtx.read_header(header_key)?;
        let body = dbtx.read_body_for_storage(header_key)?;

        let txs = dbtx
            .read_transactions(body.base_tx_id.0, body.tx_amount)?
            .map(|msg| msg.hash())
            .collect::<Vec<_>>();

        if txs.len() as u64 != body.tx_amount {
            return Err(
                format_err!("Unexpected number of transactions in block {}.", block_num).into(),
            );
        }

        let ommer_hashes = body
            .uncles
            .iter()
            .map(|header| {
                let (_, hash) = dbtx
                    .get_header_key(header.number.0)
                    .expect("no match for ommer");
                hash
            })
            .collect();

        let block = crate::utils::BlockCast(&header).cast(txs, block_num, block_hash, ommer_hashes);
        Ok(Some(block))
    }

    async fn get_block_with_txs<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<ethers::types::Transaction>>, Self::Error> {
        let mut dbtx = self.reader()?;

        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let (block_num, block_hash) = header_key;

        let header = dbtx.read_header(header_key)?;
        let body = dbtx.read_body_for_storage(header_key)?;

        let txs = dbtx
            .read_transactions(body.base_tx_id.0, body.tx_amount)?
            .scan(0_usize, |idx, msg| {
                let tx = MsgCast(&msg).cast(block_num, block_hash, *idx);
                *idx += 1;
                Some(tx)
            })
            .collect::<Vec<_>>();

        if txs.len() as u64 != body.tx_amount {
            return Err(
                format_err!("Unexpected number of transactions in block {}.", block_num).into(),
            );
        }

        let ommer_hashes = body
            .uncles
            .iter()
            .map(|header| {
                let (_, hash) = dbtx
                    .get_header_key(header.number.0)
                    .expect("no match for ommer");
                hash
            })
            .collect();

        let block = crate::utils::BlockCast(&header).cast(txs, block_num, block_hash, ommer_hashes);
        Ok(Some(block))
    }
}

#[derive(Error, Debug)]
pub enum DbMiddlewareError<M: Middleware> {
    #[error("{0}")]
    MiddlewareError(M::Error),

    #[error("{0}")]
    Anyhow(anyhow::Error),

    // placeholder error
    #[error("BadAccess")]
    BadError,
}

impl<M: Middleware> FromErr<M::Error> for DbMiddlewareError<M> {
    fn from(src: M::Error) -> DbMiddlewareError<M> {
        DbMiddlewareError::MiddlewareError(src)
    }
}
impl<M: Middleware> From<anyhow::Error> for DbMiddlewareError<M> {
    fn from(src: anyhow::Error) -> DbMiddlewareError<M> {
        DbMiddlewareError::Anyhow(src)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        account::Account,
        ffi::writer::Writer,
        tests::{get_db, TMP_DIR},
    };
    use anyhow::Result;
    use ethers::{core::types::Address, providers::Middleware, utils::keccak256};

    #[tokio::test]
    async fn test_get_balance() -> Result<()> {
        let bal = 7.into();
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2".parse()?;
        let acct = Account::new().balance(bal);

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_account(who, acct)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let res = db.get_balance(who, None).await.unwrap();
        assert_eq!(res, bal);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_transaction_count() -> Result<()> {
        let nonce = 8;
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2".parse()?;
        let acct = Account::new().nonce(nonce);

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_account(who, acct)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let res = db.get_transaction_count(who, None).await.unwrap();
        assert_eq!(res, nonce.into());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_storage_at() -> Result<()> {
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2".parse()?;
        let key = keccak256(vec![0xff]).into();
        let val = keccak256(vec![0xff, 0xab, 0xcd]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_storage(who, key, val)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let read = db.get_storage_at(who, key, None).await?;
        assert_eq!(read, val);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_block_number() -> Result<()> {
        let hash = keccak256(vec![0x10]).into();
        let num = 100;

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_head_header_hash(hash)?;
        w.put_header_number(hash, num)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let res = db.get_block_number().await?;
        assert_eq!(res, num.into());
        Ok(())
    }
}
