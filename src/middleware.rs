use anyhow::Result;
use async_trait::async_trait;
use ethers::{
    core::types::{Address, Block, BlockId, NameOrAddress, TxHash, H256, U256, U64},
    providers::{FromErr, Middleware},
};
use mdbx::EnvironmentKind;
use std::sync::Arc;
use thiserror::Error;

use crate::client::{Either, Client};

#[derive(Debug, Clone)]
pub struct DbMiddleware<M, E: EnvironmentKind> {
    inner: M,
    db: Arc<Client<E>>,
}

impl<M, E: EnvironmentKind> DbMiddleware<M, E> {
    pub fn new(inner: M, db: Arc<Client<E>>) -> Self {
        Self { inner, db }
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
        self.db.get_block_number().map_err(From::from)
    }

    async fn get_balance<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        let who = self.get_address(from).await?;
        if block.is_some() {
            return self.inner().get_balance(who, block).await.map_err(FromErr::from)
        }

        self.db.get_balance(who, block).map_err(From::from)
    }

    async fn get_code<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<ethers::types::Bytes, Self::Error> {
        let who = self.get_address(from).await?;
        if block.is_some() {
            return self.inner().get_code(who, block).await.map_err(FromErr::from)
        }

        self.db.get_code(who, block).map_err(From::from)
    }

    async fn get_transaction_count<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        let who = self.get_address(from).await?;
        if block.is_some() {
            return self.inner().get_transaction_count(who, block).await.map_err(FromErr::from)
        }

        self.db
            .get_transaction_count(who, block)
            .map_err(From::from)
    }

    async fn get_transaction<T: Send + Sync + Into<TxHash>>(
        &self,
        transaction_hash: T,
    ) -> Result<Option<ethers::types::Transaction>, Self::Error> {
        self.db
            .get_transaction(transaction_hash)
            .map_err(From::from)
    }

    async fn get_storage_at<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        location: H256,
        block: Option<BlockId>,
    ) -> Result<H256, Self::Error> {
        let who = self.get_address(from).await?;
        if block.is_some() {
            return self.inner().get_storage_at(who, location, block).await.map_err(FromErr::from)
        }

        self.db
            .get_storage_at(who, location, block)
            .map_err(From::from)
    }

    async fn get_uncle_count<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<U256, Self::Error> {
        self.db
            .get_uncle_count(block_hash_or_number)
            .map_err(From::from)
    }

    async fn get_uncle<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
        idx: U64,
    ) -> Result<Option<Block<H256>>, Self::Error> {
        self.db
            .get_uncle(block_hash_or_number, idx)
            .map_err(From::from)
    }

    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        self.db.get_block(block_hash_or_number).map_err(From::from)
    }

    async fn get_block_with_txs<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<ethers::types::Transaction>>, Self::Error> {
        self.db
            .get_block_with_txs(block_hash_or_number)
            .map_err(From::from)
    }

    async fn get_block_receipts<T: Into<ethers::types::BlockNumber> + Send + Sync>(
        &self,
        block: T,
    ) -> Result<Vec<ethers::types::TransactionReceipt>, Self::Error> {
        match self.db.get_block_receipts(block)? {
            // Receipts not in cache, delegate to inner
            Either::Left(num) => self.inner().get_block_receipts(*num).await.map_err(FromErr::from),
            // Got the receipts from the db, so return them
            Either::Right(receipts) => Ok(receipts),
        }
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
