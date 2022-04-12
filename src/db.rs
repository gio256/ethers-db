use akula::{
    kv::{
        mdbx::{MdbxEnvironment, MdbxTransaction},
        tables,
        tables::HeaderKey,
        traits::TableEncode,
    },
    models::{BlockHeader, BodyForStorage, Message, MessageWithSignature},
};
use anyhow::{bail, format_err, Result};
use async_trait::async_trait;
use ethers::{
    core::types::{Address, Block, BlockId, NameOrAddress, TxHash, H256, U256, U64},
    providers::{FromErr, Middleware},
};
use fastrlp::Decodable;
use mdbx::{EnvironmentKind, TransactionKind};
use once_cell::sync::Lazy;
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;

use crate::account::Account;

pub static EMPTY_CODEHASH: Lazy<H256> = Lazy::new(|| ethers::utils::keccak256(vec![]).into());

// A wrapper type for an Erigon MdbxTransaction
pub struct DbTx<'env, K: TransactionKind, E: EnvironmentKind>(MdbxTransaction<'env, K, E>);

fn maybe_gas_price(msg: &MessageWithSignature) -> Option<ethers::types::U256> {
    match msg.message {
        Message::Legacy { gas_price, .. } | Message::EIP2930 { gas_price, .. } => {
            Some(gas_price.to_be_bytes().into())
        }
        _ => None,
    }
}
fn tx_type(msg: &MessageWithSignature) -> Option<ethers::types::U64> {
    match msg.message {
        Message::EIP2930 { .. } => Some(1.into()),
        Message::EIP1559 { .. } => Some(2.into()),
        _ => None,
    }
}
fn access_list(
    msg: &MessageWithSignature,
) -> Option<ethers::types::transaction::eip2930::AccessList> {
    match &msg.message {
        Message::EIP2930 { access_list, .. } | Message::EIP1559 { access_list, .. } => Some(
            access_list
                .iter()
                .map(|it| ethers::types::transaction::eip2930::AccessListItem {
                    address: it.address,
                    storage_keys: it.slots.clone(),
                })
                .collect::<Vec<_>>()
                .into(),
        ),
        _ => None,
    }
}

// Most of these methods come from https://github.com/ledgerwatch/erigon/blob/f56d4c5881822e70f65927ade76ef05bfacb1df4/core/rawdb/accessors_chain.go
impl<'env, K: TransactionKind, E: EnvironmentKind> DbTx<'env, K, E> {
    pub fn new(tx: MdbxTransaction<'env, K, E>) -> Self {
        Self(tx)
    }

    /// Returns the hash of the current canonical head header.
    pub fn read_head_header_hash(&mut self) -> Result<H256> {
        self.0
            .get(
                crate::tables::LastHeader,
                String::from("LastHeader").into_bytes(),
            )
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the hash of the current canonical head block.
    pub fn read_head_block_hash(&mut self) -> Result<H256> {
        self.0
            .get(
                crate::tables::LastBlock,
                String::from("LastBlock").into_bytes(),
            )
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the header number assigned to a hash
    pub fn read_header_number(&mut self, hash: H256) -> Result<akula::models::BlockNumber> {
        self.0
            .get(tables::HeaderNumber, hash)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the number of the current canonical block header
    pub fn read_head_block_number(&mut self) -> Result<akula::models::BlockNumber> {
        let hash = self.read_head_header_hash()?;
        self.read_header_number(hash)
    }

    /// Returns the block header identified by the (block number, block hash) key
    pub fn read_header(&mut self, key: HeaderKey) -> Result<BlockHeader> {
        let raw_header = self.read_header_rlp(key)?;
        <BlockHeader as Decodable>::decode(&mut &*raw_header)
            .map_err(|e| format_err!("cant decode header: {}", e))
    }

    /// Returns the raw RLP encoded block header identified by the (block number, block hash) key
    pub fn read_header_rlp(&mut self, key: HeaderKey) -> Result<Vec<u8>> {
        self.0
            .get(akula::kv::tables::Header.erased(), key.encode().to_vec())?
            .ok_or_else(|| format_err!("cant find header"))
    }

    /// Returns the decoding of the body as stored in the BlockBody table
    pub fn read_body_for_storage(&mut self, key: HeaderKey) -> Result<BodyForStorage> {
        let raw_body = self
            .0
            .get(tables::BlockBody.erased(), key.encode().to_vec())?
            .ok_or_else(|| format_err!("cant find body"))?;

        let mut body = <akula::models::BodyForStorage as Decodable>::decode(&mut &*raw_body)
            .map_err(|e| format_err!("BodyForStorage decode error: {}", e))?;

        // https://github.com/ledgerwatch/erigon/blob/f56d4c5881822e70f65927ade76ef05bfacb1df4/core/rawdb/accessors_chain.go#L602-L605
        // Skip 1 system tx in the beginning of the block and 1 at the end
        if body.tx_amount < 2 {
            bail!(
                "block body has too few txs: {}. HeaderKey: {:?}",
                body.tx_amount,
                key
            )
        }
        body.base_tx_id.0 += 1;
        body.tx_amount -= 2;

        Ok(body)
    }

    /// Returns the number of the block containing the specified transaction.
    pub fn read_transaction_block_number(
        &mut self,
        hash: H256,
    ) -> Result<akula::models::BlockNumber> {
        let num = self
            .0
            .get(crate::tables::BlockTransactionLookup, hash)?
            .ok_or_else(|| format_err!("cant find tx"))?;

        if num > akula::models::U256::from(u64::MAX) {
            bail!("Invalid block number from TxLookup: {:?}", num)
        }

        Ok(num.as_u64().into())
    }

    /// Returns an iterator over the `n` transactions beginning at `start_key`.
    pub fn read_transactions(
        &mut self,
        start_key: u64,
        n: u64,
    ) -> Result<impl Iterator<Item = MessageWithSignature>> {
        // Note: The BlockTransaction table is called "EthTx" in erigon
        Ok(self
            .0
            .cursor(tables::BlockTransaction.erased())?
            .walk(Some(start_key.encode().to_vec()))
            .flat_map(|res| {
                res.and_then(|(_, tx)| {
                    Ok(<akula::models::MessageWithSignature as Decodable>::decode(
                        &mut &*tx,
                    )?)
                })
            })
            .take(n.try_into()?))
    }

    /// Returns the hash assigned to a canonical block number.
    pub fn read_canonical_hash(&mut self, num: akula::models::BlockNumber) -> Result<H256> {
        self.0
            .get(akula::kv::tables::CanonicalHeader, num)
            .map(|res| res.unwrap_or_default())
    }

    /// Determines whether a header with the given hash is on the canonical chain.
    pub fn is_canonical_hash(&mut self, hash: H256) -> Result<bool> {
        let num = self.read_header_number(hash)?;
        let canonical_hash = self.read_canonical_hash(num)?;
        Ok(canonical_hash != Default::default() && canonical_hash == hash)
    }

    /// Returns the decoded account data as stored in the PlainState table.
    pub fn read_account_data(&mut self, who: Address) -> Result<Account> {
        self.0
            .get(crate::tables::PlainState, who)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the value of the storage for account `who` indexed by `key`.
    pub fn read_account_storage(
        &mut self,
        who: Address,
        incarnation: u64,
        key: H256,
    ) -> Result<H256> {
        let bucket = crate::tables::StorageBucket::new(who, incarnation);
        let mut cur = self.0.cursor(crate::tables::Storage)?;

        if let Some((k, v)) = cur.seek_both_range(bucket, key)? {
            if k == key {
                return Ok(v.to_be_bytes().into());
            }
        }

        Ok(Default::default())
    }

    /// Returns the incarnation of the account when it was last deleted
    pub fn read_last_incarnation(&mut self, who: Address) -> Result<u64> {
        self.0
            .get(crate::tables::IncarnationMap, who)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the code associated with the given codehash.
    pub fn read_code(&mut self, codehash: H256) -> Result<bytes::Bytes> {
        if codehash == *EMPTY_CODEHASH {
            return Ok(bytes::Bytes::new());
        }
        self.0
            .get(tables::Code, codehash)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the code associated with the given codehash.
    pub fn read_code_size(&mut self, codehash: H256) -> Result<usize> {
        let code = self.read_code(codehash)?;
        Ok(code.len())
    }

    pub fn get_header_key<T: Into<BlockId> + Send + Sync>(&mut self, id: T) -> Result<HeaderKey> {
        let (num, hash) = match id.into() {
            BlockId::Hash(hash) => {
                let num = self.read_header_number(hash)?.0.into();
                (num, hash)
            }
            BlockId::Number(id) => match id {
                ethers::core::types::BlockNumber::Number(n) => {
                    (n, self.read_canonical_hash(n.as_u64().into())?)
                }
                ethers::core::types::BlockNumber::Latest => {
                    let hash = self.read_head_header_hash()?;
                    let num = self.read_header_number(hash)?;
                    (num.0.into(), hash)
                }
                _ => panic!("unsupported block id type"),
            },
        };
        Ok((num.as_u64().into(), hash))
    }
}

// The actual middleware
#[derive(Debug)]
pub struct Db<M, E: EnvironmentKind> {
    inner: M,
    db: Arc<MdbxEnvironment<E>>,
}

pub fn open_db<E: EnvironmentKind>(chaindata_dir: PathBuf) -> Result<MdbxEnvironment<E>> {
    MdbxEnvironment::<E>::open_ro(
        mdbx::Environment::new(),
        &chaindata_dir,
        // opening read-only, so the size of the DatabaseChat determines max_dbs,
        // but the contents are discarded
        akula::kv::tables::CHAINDATA_TABLES.clone(),
    )
}

impl<M, E: EnvironmentKind> Db<M, E> {
    pub fn open_new(inner: M, chaindata_dir: PathBuf) -> Result<Self> {
        let db = open_db(chaindata_dir)?;
        Ok(Self {
            inner,
            db: Arc::new(db),
        })
    }

    pub fn new(inner: M, db: Arc<MdbxEnvironment<E>>) -> Self {
        Self { inner, db }
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
        let mut dbtx = DbTx::new(self.db.begin()?);
        Ok(dbtx.read_head_block_number()?.0.into())
    }

    async fn get_balance<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        let mut dbtx = DbTx::new(self.db.begin()?);
        Ok(dbtx.read_account_data(who)?.balance)
    }

    async fn get_transaction_count<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        block: Option<BlockId>,
    ) -> Result<U256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;
        let mut dbtx = DbTx::new(self.db.begin()?);
        Ok(dbtx.read_account_data(who)?.nonce.into())
    }

    async fn get_storage_at<T: Into<NameOrAddress> + Send + Sync>(
        &self,
        from: T,
        location: H256,
        block: Option<BlockId>,
    ) -> Result<H256, Self::Error> {
        assert!(block.is_none(), "no history handling yet");
        let who = self.get_address(from).await?;

        let mut dbtx = DbTx::new(self.db.begin()?);
        let acct = dbtx.read_account_data(who)?;
        dbtx.read_account_storage(who, acct.incarnation, location)
            .map_err(From::from)
    }

    // https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/bin/akula.rs#L266
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let mut dbtx = DbTx::new(self.db.begin()?);

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

    async fn get_block_with_txs<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<ethers::types::Transaction>>, Self::Error> {
        let mut dbtx = DbTx::new(self.db.begin()?);

        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let (block_num, block_hash) = header_key;

        let header = dbtx.read_header(header_key)?;
        let body = dbtx.read_body_for_storage(header_key)?;

        let txs = dbtx
            .read_transactions(body.base_tx_id.0, body.tx_amount)?
            .scan(0_u64, |idx, msg| {
                let tx = ethers::types::Transaction {
                    hash: msg.hash(),
                    nonce: msg.nonce().into(),
                    block_hash: Some(block_hash),
                    block_number: Some(block_num.0.into()),
                    transaction_index: Some((*idx).into()),
                    from: msg.recover_sender().expect("bad sig"), //TODO: erigon has a separate table they merge instead
                    to: msg.action().into_address(),
                    value: msg.value().to_be_bytes().into(),
                    gas_price: maybe_gas_price(&msg),
                    gas: msg.gas_limit().into(),
                    input: msg.input().clone().into(),
                    v: msg.v().into(),
                    r: msg.r().to_fixed_bytes().into(),
                    s: msg.s().to_fixed_bytes().into(),
                    transaction_type: tx_type(&msg),
                    access_list: access_list(&msg),
                    chain_id: msg.chain_id().map(|id| id.0.into()),

                    //TODO: should these be None for legacy txs?
                    max_priority_fee_per_gas: Some(msg.max_priority_fee_per_gas().to_be_bytes().into()),
                    max_fee_per_gas: Some(msg.max_fee_per_gas().to_be_bytes().into()),
                };
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
