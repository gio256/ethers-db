use akula::{
    kv::{mdbx::MdbxEnvironment, tables, traits::TableEncode},
    models::{BlockBody, BlockHeader},
};
use async_trait::async_trait;
use ethers::core::types::{Address, Block, BlockId, BlockNumber, NameOrAddress, TxHash, U256, U64};
use ethers::providers::{maybe, FromErr, Middleware, PendingTransaction, ProviderError};
use eyre::{eyre, Result};
use fastrlp::Decodable;
use mdbx::EnvironmentKind;
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;

use crate::account::Account;

#[derive(Debug)]
pub struct Db<M, E: EnvironmentKind> {
    inner: M,
    db: Arc<MdbxEnvironment<E>>,
}

impl<M, E: EnvironmentKind> Db<M, E> {
    pub fn new(inner: M, data_dir: PathBuf) -> Result<Self> {
        let chaindata_dir = data_dir.join("chaindata");
        let db = MdbxEnvironment::<E>::open_ro(
            mdbx::Environment::new(),
            &chaindata_dir,
            akula::kv::tables::CHAINDATA_TABLES.clone(),
        )
        .map_err(|e| eyre!("Chaindata error: {}", e))?;
        Ok(Self {
            inner,
            db: Arc::new(db),
        })
    }
}

impl<M, E> Db<M, E>
where
    M: Middleware,
    E: EnvironmentKind,
{
    pub fn get_account(
        &self,
        who: Address,
    ) -> Result<Option<Account>, <Self as Middleware>::Error> {
        let tx = self.db.begin()?;
        let raw = tx.get(crate::tables::PlainState, who)?.unwrap_or_default();
        Account::decode_for_storage(&raw).map_err(From::from)
    }

    async fn get_address<T: Into<NameOrAddress>>(
        &self,
        who: T,
    ) -> Result<Address, <Self as Middleware>::Error> {
        match who.into() {
            NameOrAddress::Name(name) => self.resolve_name(&name).await,
            NameOrAddress::Address(adr) => Ok(adr),
        }
    }

    async fn get_header<T: Into<BlockId> + Send + Sync>(
        &self,
        id: T,
    ) -> Result<Option<BlockHeader>, <Self as Middleware>::Error> {
        let tx = self.db.begin()?;

        let last = tx.get(akula::kv::tables::LastHeader, ()).unwrap();
        dbg!(last);
        let (num, hash) = match id.into() {
            BlockId::Hash(_h) => panic!(""),
            BlockId::Number(id) => match id {
                BlockNumber::Number(n) => (
                    n,
                    tx.get(akula::kv::tables::CanonicalHeader, n.as_u64().into())
                        .unwrap()
                        .unwrap(),
                ),
                _ => panic!("unsupported block id type"),
            },
        };
        dbg!(hash);
        dbg!(num);
        let key = TableEncode::encode((num.as_u64(), hash)).to_vec();
        let raw_header = tx
            .get(akula::kv::tables::Header.erased(), key.clone())
            .unwrap()
            .unwrap();
        let header = <BlockHeader as Decodable>::decode(&mut &*raw_header).unwrap();
        Ok(Some(header))
    }
}

#[async_trait]
impl<M, E> Middleware for Db<M, E>
where
    M: Middleware,
    E: EnvironmentKind,
{
    type Error = DbError<M>;
    type Provider = M::Provider;
    type Inner = M;

    fn inner(&self) -> &Self::Inner {
        &self.inner
    }

    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        let tx = self.db.begin()?;
        let hash = tx
            .get(
                crate::tables::LastBlock,
                String::from("LastBlock").into_bytes(),
            )?
            .ok_or(DbError::BadError)?;
        let num = tx
            .get(tables::HeaderNumber, hash)?
            .ok_or(DbError::BadError)?;
        Ok(num.0.into())
    }

    async fn get_balance<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        Ok(self
            .get_account(who)?
            .map_or_else(|| Default::default(), |acct| acct.balance))
    }

    async fn get_transaction_count<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        Ok(self
            .get_account(who)?
            .map_or_else(|| Default::default(), |acct| acct.nonce.into()))
    }

    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let tx = self.db.begin()?;

        let block = akula::models::BlockNumber(2);

        let cur = tx.cursor(akula::kv::tables::BlockBody.erased()).unwrap();
        let mut walker = cur.walk(Some(TableEncode::encode(block).to_vec()));

        use akula::kv::traits::TableDecode;
        let (k, v) = walker.next().transpose().unwrap().unwrap();
        let (num, hash) = <(akula::models::BlockNumber, akula::models::H256)>::decode(&k).unwrap();
        dbg!(num);
        dbg!(hash);

        let body = <akula::models::BodyForStorage as Decodable>::decode(&mut &*v).unwrap();
        let body = dbg!(body);

        let base_tx_id = body.base_tx_id;
        let tx_amt = usize::try_from(body.tx_amount).unwrap();
        let txs = tx
            .cursor(akula::kv::tables::BlockTransaction.erased())
            .unwrap()
            .walk(Some(base_tx_id.encode().to_vec()))
            .map(|res| res.map(|(_, tx)| tx))
            .take(tx_amt)
            .collect::<Vec<_>>();

        dbg!(txs);

        // let raw_body = tx.get(tables::BlockBody.erased(), key).unwrap().unwrap();
        // dbg!(hex::encode(raw_body.clone()));
        // dbg!(hex::encode(body.clone()));
        // dbg!(body);

        // let body = <BlockBody as Decodable>::decode(&mut &*raw_body).unwrap();
        // dbg!(body);

        panic!("");
        Err(Self::Error::BadError)
    }
}

#[derive(Error, Debug)]
pub enum DbError<M: Middleware> {
    #[error("{0}")]
    MiddlewareError(M::Error),

    #[error("{0}")]
    Anyhow(anyhow::Error),

    #[error("{0}")]
    Eyre(eyre::ErrReport),

    // placeholder error
    #[error("BadAccess")]
    BadError,
}

impl<M: Middleware> FromErr<M::Error> for DbError<M> {
    fn from(src: M::Error) -> DbError<M> {
        DbError::MiddlewareError(src)
    }
}
impl<M: Middleware> From<anyhow::Error> for DbError<M> {
    fn from(src: anyhow::Error) -> DbError<M> {
        DbError::Anyhow(src)
    }
}
impl<M: Middleware> From<eyre::ErrReport> for DbError<M> {
    fn from(src: eyre::ErrReport) -> DbError<M> {
        DbError::Eyre(src)
    }
}
