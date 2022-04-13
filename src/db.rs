#![allow(dead_code)]

use akula::{
    kv::{mdbx::MdbxTransaction, tables, tables::HeaderKey, traits::TableEncode},
    models::{BlockHeader, BodyForStorage, MessageWithSignature},
};
use anyhow::{format_err, Result};
use ethers::core::types::{Address, BlockId, H256};
use fastrlp::Decodable;
use mdbx::{EnvironmentKind, TransactionKind};
use once_cell::sync::Lazy;

use crate::account::Account;

pub static EMPTY_CODEHASH: Lazy<H256> = Lazy::new(|| ethers::utils::keccak256(vec![]).into());

/// A Reader wraps an MdbxTransaction and provides Erigon-specific access methods.
pub struct Reader<'env, K: TransactionKind, E: EnvironmentKind>(MdbxTransaction<'env, K, E>);

// Most of these methods are ported from erigon/core/rawdb/accesssors_*.go
impl<'env, K: TransactionKind, E: EnvironmentKind> Reader<'env, K, E> {
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

        // Skip 1 system tx at the beginning of the block and 1 at the end
        // https://github.com/ledgerwatch/erigon/blob/f56d4c5881822e70f65927ade76ef05bfacb1df4/core/rawdb/accessors_chain.go#L602-L605
        body.base_tx_id.0 += 1;
        body.tx_amount = body.tx_amount.checked_sub(2).ok_or_else(|| {
            format_err!(
                "Block body has too few txs: {}. HeaderKey: {:?}",
                body.tx_amount,
                key,
            )
        })?;

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

        Ok(u64::try_from(num)?.into())
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
        let bucket = crate::storage::StorageBucket::new(who, incarnation);
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
