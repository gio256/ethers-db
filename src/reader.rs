#![allow(dead_code)]

use akula::{
    kv::{mdbx::MdbxTransaction, tables as ak_tables, traits::TableEncode},
    models as ak_models,
};
use anyhow::{format_err, Result};
use ethers::core::types::{Address, BlockId, BlockNumber as EthersBlockNumber, H256};
use fastrlp::Decodable;
use mdbx::{EnvironmentKind, TransactionKind};
use once_cell::sync::Lazy;

use crate::{account::Account, tables};

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
            .get(tables::LastHeader, String::from("LastHeader").into_bytes())
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the hash of the current canonical head block.
    pub fn read_head_block_hash(&mut self) -> Result<H256> {
        self.0
            .get(tables::LastBlock, String::from("LastBlock").into_bytes())
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the header number assigned to a hash
    pub fn read_header_number(&mut self, hash: H256) -> Result<ak_models::BlockNumber> {
        self.0
            .get(ak_tables::HeaderNumber, hash)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the number of the current canonical block header
    pub fn read_head_block_number(&mut self) -> Result<ak_models::BlockNumber> {
        let hash = self.read_head_header_hash()?;
        self.read_header_number(hash)
    }

    /// Returns the block header identified by the (block number, block hash) key
    pub fn read_header(&mut self, key: ak_tables::HeaderKey) -> Result<ak_models::BlockHeader> {
        let raw_header = self.read_header_rlp(key)?;
        <ak_models::BlockHeader as Decodable>::decode(&mut &*raw_header)
            .map_err(|e| format_err!("cant decode header: {}", e))
    }

    /// Returns the raw RLP encoded block header identified by the (block number, block hash) key
    pub fn read_header_rlp(&mut self, key: ak_tables::HeaderKey) -> Result<Vec<u8>> {
        self.0
            .get(ak_tables::Header.erased(), key.encode().to_vec())?
            .ok_or_else(|| format_err!("cant find header"))
    }

    /// Returns the decoding of the body as stored in the BlockBody table
    pub fn read_body_for_storage(
        &mut self,
        key: ak_tables::HeaderKey,
    ) -> Result<ak_models::BodyForStorage> {
        let raw_body = self
            .0
            .get(ak_tables::BlockBody.erased(), key.encode().to_vec())?
            .ok_or_else(|| format_err!("cant find body"))?;

        let mut body = <ak_models::BodyForStorage as Decodable>::decode(&mut &*raw_body)
            .map_err(|e| format_err!("BodyForStorage decode error: {}", e))?;

        //TODO move this into read_body
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
    pub fn read_transaction_block_number(&mut self, hash: H256) -> Result<ak_models::BlockNumber> {
        let num = self
            .0
            .get(tables::BlockTransactionLookup, hash)?
            .ok_or_else(|| format_err!("cant find tx"))?;

        Ok(u64::try_from(num)?.into())
    }

    /// Returns an iterator over the `n` transactions beginning at `start_key`.
    pub fn read_transactions(
        &mut self,
        start_key: u64,
        n: u64,
    ) -> Result<impl Iterator<Item = ak_models::MessageWithSignature>> {
        // BlockTransaction is Erigon's "EthTx" table
        Ok(self
            .0
            .cursor(ak_tables::BlockTransaction.erased())?
            .walk(Some(start_key.encode().to_vec()))
            .flat_map(|res| {
                res.and_then(|(_, tx)| {
                    //TODO: decoding issues?
                    Ok(<ak_models::MessageWithSignature as Decodable>::decode(
                        &mut &*tx,
                    )?)
                })
            })
            .take(n.try_into()?))
    }

    /// Returns the hash assigned to a canonical block number.
    pub fn read_canonical_hash(&mut self, num: ak_models::BlockNumber) -> Result<H256> {
        self.0
            .get(ak_tables::CanonicalHeader, num)
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
            .get(tables::PlainState, who)
            .map(|res| res.unwrap_or_default())
    }

    pub fn read_account_data_raw(&mut self, who: Address) -> Result<Vec<u8>> {
        self.0
            .get(tables::PlainState.erased(), who.encode().to_vec())
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
        let mut cur = self.0.cursor(tables::Storage)?;

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
            .get(tables::IncarnationMap, who)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the code associated with the given codehash.
    pub fn read_code(&mut self, codehash: H256) -> Result<bytes::Bytes> {
        if codehash == *EMPTY_CODEHASH {
            return Ok(bytes::Bytes::new());
        }
        self.0
            .get(ak_tables::Code, codehash)
            .map(|res| res.unwrap_or_default())
    }

    /// Returns the code associated with the given codehash.
    pub fn read_code_size(&mut self, codehash: H256) -> Result<usize> {
        let code = self.read_code(codehash)?;
        Ok(code.len())
    }

    /// Returns the (block number, block hash) key used to identify a block in the db
    pub fn get_header_key<T: Into<BlockId> + Send + Sync>(
        &mut self,
        id: T,
    ) -> Result<ak_tables::HeaderKey> {
        let (num, hash) = match id.into() {
            BlockId::Hash(hash) => {
                let num = self.read_header_number(hash)?.0.into();
                (num, hash)
            }
            BlockId::Number(id) => match id {
                EthersBlockNumber::Number(n) => (n, self.read_canonical_hash(n.as_u64().into())?),
                //TODO: check this https://github.com/ledgerwatch/erigon/blob/156da607e7495d709c141aec40f66a2556d35dc0/cmd/rpcdaemon/commands/rpc_block.go#L30
                EthersBlockNumber::Latest | EthersBlockNumber::Pending => {
                    let hash = self.read_head_header_hash()?;
                    let num = self.read_header_number(hash)?;
                    (num.0.into(), hash)
                }
                EthersBlockNumber::Earliest => (0.into(), self.read_canonical_hash(0.into())?),
            },
        };
        Ok((num.as_u64().into(), hash))
    }

    /// Helper fn to walk a db table and print key, value pairs
    pub fn walk_table_debug<T: akula::kv::Table>(
        &mut self,
        table: ak_tables::ErasedTable<T>,
    ) -> Result<()> {
        println!("\nWalking table: {:?}", table.0);
        let mut cur = self.0.cursor(table).unwrap();
        while let Some((k, v)) = cur.next().unwrap() {
            let k = hex::encode(k);
            let v = hex::encode(v);
            println!("key: {:?}\nval: {:?}\n", k, v);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use akula::models::{self as ak_models, BodyForStorage, MessageWithSignature, H256};
    use anyhow::Result;
    use ethers::{core::types::Address, utils::keccak256};
    use rand::thread_rng;
    use std::path::PathBuf;

    use crate::{
        account::Account, client::Client, ffi::writer::Writer, rand::Rand, tests::TMP_DIR,
    };

    // helper for type inference
    pub fn client(path: PathBuf) -> Result<Client<mdbx::NoWriteMap>> {
        Client::open_new(path)
    }

    #[test]
    fn test_read_head_header_hash() -> Result<()> {
        let hash = keccak256(vec![0xab]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_head_header_hash(hash)?;
        let path = w.close()?;

        let db = client(path)?;
        let read = db.reader()?.read_head_header_hash()?;
        assert_eq!(read, hash);
        Ok(())
    }

    #[test]
    fn test_read_header_number() -> Result<()> {
        let mut rng = thread_rng();
        let num = Rand::rand(&mut rng);
        let hash = keccak256(vec![0x10]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_header_number(hash, num)?;
        let path = w.close()?;

        let db = client(path)?;
        let read = db.reader()?.read_header_number(hash)?;
        assert_eq!(read, num);
        Ok(())
    }

    #[test]
    fn test_is_canonical_hash() -> Result<()> {
        let mut rng = thread_rng();
        let num = Rand::rand(&mut rng);
        let hash = keccak256(vec![0x10]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_header_number(hash, num)?;
        w.put_canonical_hash(hash, num)?;
        let path = w.close()?;

        let db = client(path)?;
        let read = db.reader()?.is_canonical_hash(hash)?;
        assert!(read);
        Ok(())
    }

    #[test]
    fn test_account_accessor() -> Result<()> {
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2".parse()?;
        let acct = Account {
            nonce: 1,
            incarnation: 2,
            balance: ethers::types::U256::MAX,
            codehash: keccak256(vec![0xff]).into(),
        };

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_account(who, acct)?;
        let path = w.close()?;

        let db = client(path)?;
        let mut dbtx = db.reader().unwrap();
        let read = dbtx.read_account_data(who).unwrap();
        assert_eq!(acct, read);
        Ok(())
    }

    #[test]
    fn test_read_transactions() -> Result<()> {
        let mut rng = thread_rng();
        let base_id = u64::rand(&mut rng);
        let n = 3;

        let txs = (0..n)
            .map(|_| MessageWithSignature::rand(&mut rng))
            .collect::<Vec<_>>();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_transactions(txs.clone(), base_id)?;
        let path = w.close()?;

        let db = client(path)?;
        let mut dbtx = db.reader().unwrap();
        let read = dbtx.read_transactions(base_id, n).unwrap();

        for (i, t) in read.enumerate() {
            assert_eq!(t, txs[i]);
        }
        Ok(())
    }

    #[test]
    fn test_read_body_for_storage() -> Result<()> {
        let mut rng = thread_rng();
        let hash = H256::rand(&mut rng);
        let num = u64::rand(&mut rng);
        let body = BodyForStorage::rand(&mut rng);

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_body_for_storage(hash, num.into(), body.clone())?;
        let path = w.close()?;

        let db = client(path)?;
        let mut dbtx = db.reader().unwrap();
        let key = (num.into(), hash);
        let read = dbtx.read_body_for_storage(key).unwrap();

        assert_eq!(read.base_tx_id, body.base_tx_id + 1);
        assert_eq!(read.tx_amount + 2, body.tx_amount);
        assert_eq!(read.uncles, body.uncles);
        Ok(())
    }

    #[test]
    fn test_read_transaction_block_number() -> Result<()> {
        let mut rng = thread_rng();
        let block_num = ak_models::BlockNumber::rand(&mut rng);
        let tx_hashes = (0..5).map(|_| H256::rand(&mut rng)).collect::<Vec<_>>();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_tx_lookup_entries(block_num, tx_hashes.clone())?;
        let path = w.close()?;

        let db = client(path)?;
        let mut dbtx = db.reader().unwrap();
        for hash in tx_hashes {
            let read = dbtx.read_transaction_block_number(hash).unwrap();
            assert_eq!(read, block_num);
        }
        Ok(())
    }
}
