use akula::{
    kv::{
        mdbx::{MdbxEnvironment, MdbxTransaction},
        tables,
        tables::HeaderKey,
        traits::TableEncode,
    },
    models::{Message, BodyForStorage, BlockBody, BlockHeader},
};
use anyhow::{bail, format_err};
use async_trait::async_trait;
use ethers::{
    core::types::{Transaction, Address, Block, BlockId, BlockNumber, NameOrAddress, TxHash, H256, U256, U64},
    providers::{maybe, FromErr, Middleware, PendingTransaction, ProviderError},
};
use eyre::{eyre, Result};
use fastrlp::Decodable;
use mdbx::EnvironmentKind;
use std::{borrow::Borrow, path::PathBuf, sync::Arc};
use thiserror::Error;

use crate::account::Account;

//TODO:
// historical state
// propogate db better

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

    pub fn get_account(&self, who: Address) -> anyhow::Result<Option<Account>> {
        let tx = self.db.begin()?;
        tx.get(crate::tables::PlainState, who)
    }

    pub fn read_header(
        &self,
        tx: &mut MdbxTransaction<'_, mdbx::RO, E>,
        key: HeaderKey,
    ) -> anyhow::Result<BlockHeader> {
        let raw_header = tx
            .get(akula::kv::tables::Header.erased(), key.encode().to_vec())?
            .ok_or_else(|| format_err!("cant find header"))?;
        <BlockHeader as Decodable>::decode(&mut &*raw_header)
            .map_err(|e| format_err!("cant decode header: {}", e))
    }

    pub fn read_body(
        &self,
        tx: &mut MdbxTransaction<'_, mdbx::RO, E>,
        key: HeaderKey,
    ) -> anyhow::Result<BodyForStorage> {
        let raw_body = tx
            .get(
                tables::BlockBody.erased(),
                key.encode().to_vec(),
            )?
            .ok_or_else(|| format_err!("cant find body"))?;

        let body = <akula::models::BodyForStorage as Decodable>::decode(&mut &*raw_body)
            .map_err(|e| format_err!("BodyForStorage decode error: {}", e))?;

        Ok(body)
    }

    pub fn read_transactions(
        &self,
        dbtx: &mut MdbxTransaction<'_, mdbx::RO, E>,
        body: &BodyForStorage,
    ) -> anyhow::Result<impl Iterator<Item = Message>> {
        let tx_amount = usize::try_from(body.tx_amount)
            .map_err(|e| format_err!("Bad BodyForStorage tx_amount: {}", e))?;

        // https://github.com/ledgerwatch/erigon/blob/f56d4c5881822e70f65927ade76ef05bfacb1df4/core/rawdb/accessors_chain.go#L602-L604
        if tx_amount < 2 {
            panic!("block body has unexpected tx_amount: {:?}", body.tx_amount)
        }

        // Note: BlockTransaction is EthTx in erigon
        // https://github.com/ledgerwatch/erigon-lib/blob/da0666bd83faf7879a2a0c2a814a94965f78883b/kv/tables.go#L227
        Ok(dbtx
            .cursor(tables::BlockTransaction.erased())?
            .walk(Some(body.base_tx_id.encode().to_vec()))
            .map(|res| {
                res.and_then(|(_, tx)| {
                    Ok(<akula::models::MessageWithSignature as Decodable>::decode(&mut &*tx)?.message)
                })
            })
            .flatten()
            .take(tx_amount))
    }

}

impl<M, E> Db<M, E>
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

    pub fn get_header_key<T: Into<BlockId> + Send + Sync>(
        &self,
        id: T,
    ) -> Result<HeaderKey, <Self as Middleware>::Error> {
        let tx = self.db.begin()?;

        let (num, hash) = match id.into() {
            BlockId::Hash(_h) => {
                let num = tx.get(tables::HeaderNumber, _h)?.ok_or(DbError::BadError)?;
                (num.0.into(), _h)
            }
            BlockId::Number(id) => match id {
                BlockNumber::Number(n) => (
                    n,
                    tx.get(akula::kv::tables::CanonicalHeader, n.as_u64().into())
                        .unwrap()
                        .unwrap(),
                ),
                BlockNumber::Latest => {
                    let hash = tx
                        .get(
                            crate::tables::LastHeader,
                            String::from("LastHeader").into_bytes(),
                        )?
                        .ok_or(DbError::BadError)?;
                    let num = tx
                        .get(tables::HeaderNumber, hash)?
                        .ok_or(DbError::BadError)?;
                    (num.0.into(), hash)
                }
                _ => panic!("unsupported block id type"),
            },
        };
        Ok((num.as_u64().into(), hash))
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
                crate::tables::LastHeader,
                String::from("LastHeader").into_bytes(),
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
        let tx = self.db.begin()?;
        Ok(tx
            .get(crate::tables::PlainState, who)?
            .map_or_else(|| Default::default(), |acct| acct.balance))
    }

    async fn get_transaction_count<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        let tx = self.db.begin()?;
        Ok(tx
            .get(crate::tables::PlainState, who)?
            .map_or_else(|| Default::default(), |acct| acct.nonce.into()))
    }

    async fn get_storage_at<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        location: H256,
        block: Option<BlockId>,
    ) -> Result<H256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;

        let tx = self.db.begin()?;
        let tx = self.db.begin()?;
        let acct = tx.get(crate::tables::PlainState, who)?.unwrap_or_default();

        let bucket = crate::tables::StorageBucket::new(who, acct.incarnation);
        let mut cur = tx.cursor(crate::tables::Storage).unwrap();

        if let Some((k, v)) = cur.seek_both_range(bucket, location)? {
            if k == location {
                return Ok(v.to_be_bytes().into());
            }
        }

        Ok(Default::default())
    }

    // https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/bin/akula.rs#L266
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let header_key = self.get_header_key(block_hash_or_number)?;
        let (block_num, block_hash) = header_key;

        let mut tx = self.db.begin()?;

        let header = self.read_header(&mut tx, header_key)?;
        let body = self.read_body(&mut tx, header_key)?;

        let txs = self.read_transactions(&mut tx, &body)?.map(|msg| msg.hash()).collect::<Vec<_>>();

        if txs.len() as u64 != body.tx_amount {
            return Err(eyre!("Unexpected number of transactions in block {}.", block_num).into());
        }

        let ommer_hashes = body
            .uncles
            .iter()
            .map(|header| {
                let (_, hash) = self
                    .get_header_key(header.number.0)
                    .expect("no match for ommer");
                hash
            })
            .collect();

        let block = Block {
            hash: Some(block_hash),
            parent_hash: header.parent_hash,
            uncles_hash: header.ommers_hash,
            author: header.beneficiary,
            state_root: header.state_root,
            transactions_root: header.transactions_root,
            receipts_root: header.receipts_root,
            number: Some(block_num.0.into()),
            gas_used: header.gas_used.into(),
            gas_limit: header.gas_limit.into(),
            extra_data: header.extra_data.into(),
            logs_bloom: Some(header.logs_bloom),
            timestamp: header.timestamp.into(),
            difficulty: header.difficulty.to_be_bytes().into(),
            total_difficulty: None, // TODO
            uncles: ommer_hashes,
            transactions: txs,
            mix_hash: Some(header.mix_hash),
            nonce: Some(header.nonce.to_fixed_bytes().into()),
            base_fee_per_gas: header.base_fee_per_gas.map(|f| f.to_be_bytes().into()),

            // TODO:
            // seal_fields
            //size
            ..Default::default()
        };
        Ok(Some(block))
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
