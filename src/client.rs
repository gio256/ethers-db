use akula::kv::mdbx::MdbxEnvironment;
use anyhow::{format_err, Result};
use ethers::core::types::{Address, Block, BlockId, TxHash, H256, U256, U64};
use mdbx::EnvironmentKind;
use std::path::PathBuf;

use crate::reader::Reader;
use crate::utils::{open_db, MsgCast};

#[derive(Debug)]
pub struct Client<E: EnvironmentKind>(MdbxEnvironment<E>);

impl<E: EnvironmentKind> Client<E> {
    pub fn new(db: MdbxEnvironment<E>) -> Self {
        Self(db)
    }

    pub fn open_new(chaindata_dir: PathBuf) -> Result<Self> {
        let db = open_db(chaindata_dir)?;
        Ok(Self(db))
    }

    pub fn reader(&self) -> Result<Reader<'_, mdbx::RO, E>> {
        Ok(Reader::new(self.0.begin()?))
    }
}

// Synchronous middleware methods
impl<E: EnvironmentKind> Client<E> {
    pub fn get_block_number(&self) -> Result<U64> {
        let mut dbtx = self.reader()?;
        Ok(dbtx.read_head_block_number()?.0.into())
    }

    pub fn get_balance(&self, from: Address, block: Option<BlockId>) -> Result<U256> {
        assert!(block.is_none(), "no history handling yet");
        let mut dbtx = self.reader()?;
        Ok(dbtx.read_account_data(from)?.balance)
    }

    pub fn get_transaction_count(&self, from: Address, block: Option<BlockId>) -> Result<U256> {
        assert!(block.is_none(), "no history handling yet");
        let mut dbtx = self.reader()?;
        Ok(dbtx.read_account_data(from)?.nonce.into())
    }

    pub fn get_transaction<T: Send + Sync + Into<TxHash>>(
        &self,
        transaction_hash: T,
    ) -> Result<Option<ethers::types::Transaction>> {
        let hash = transaction_hash.into();

        let mut dbtx = self.reader()?;
        let block_num = dbtx.read_transaction_block_number(hash)?;
        let block_hash = dbtx.read_canonical_hash(block_num)?;
        let body = dbtx.read_body_for_storage((block_num, block_hash))?;

        //TODO read sender from db
        let (msg, idx) = dbtx
            .read_transactions(body.base_tx_id.0, body.tx_amount)?
            .zip(0..)
            .find(|(msg, _i)| msg.hash() == hash)
            .unwrap();

        Ok(Some(MsgCast(&msg).cast(block_num, block_hash, idx)))
    }

    pub fn get_storage_at(
        &self,
        from: Address,
        location: H256,
        block: Option<BlockId>,
    ) -> Result<H256> {
        assert!(block.is_none(), "no history handling yet");
        let mut dbtx = self.reader()?;
        let acct = dbtx.read_account_data(from)?;
        dbtx.read_account_storage(from, acct.incarnation, location)
            .map_err(From::from)
    }

    pub fn get_uncle_count<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<U256> {
        let mut dbtx = self.reader()?;
        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let body = dbtx.read_body_for_storage(header_key)?;
        Ok(body.uncles.len().into())
    }

    pub fn get_uncle<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
        idx: U64,
    ) -> Result<Option<Block<H256>>> {
        let mut dbtx = self.reader()?;
        let header_key = dbtx.get_header_key(block_hash_or_number)?;
        let body = dbtx.read_body_for_storage(header_key)?;
        let idx = idx.as_usize();
        if idx < body.uncles.len() {
            self.get_block(body.uncles[idx].number.0)
        } else {
            Ok(None)
        }
    }

    // https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/bin/akula.rs#L266
    pub fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>> {
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

    pub fn get_block_with_txs<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<ethers::types::Transaction>>> {
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ethers::{core::types::Address, utils::keccak256};
    use std::path::PathBuf;
    use akula::models::{BodyForStorage, MessageWithSignature, H256};

    use super::Client;
    use crate::{account::Account, ffi::writer::Writer, tests::TMP_DIR, rand::Rand, utils::MsgCast};
    use rand::thread_rng;

    // helper for type inference
    pub fn client(path: PathBuf) -> Result<Client<mdbx::NoWriteMap>> {
        Client::open_new(path)
    }

    #[test]
    fn test_get_balance() -> Result<()> {
        let mut rng = thread_rng();
        let who = Rand::rand(&mut rng);
        let bal = <[u8; 32]>::rand(&mut rng).into();
        let acct = Account::new().balance(bal);

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_account(who, acct)?;
        let path = w.close()?;

        let db = client(path)?;
        let res = db.get_balance(who, None).unwrap();
        assert_eq!(res, bal);
        Ok(())
    }

    #[test]
    fn test_get_transaction_count() -> Result<()> {
        let mut rng = thread_rng();
        let who = Rand::rand(&mut rng);
        let nonce = Rand::rand(&mut rng);
        let acct = Account::new().nonce(nonce);

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_account(who, acct)?;
        let path = w.close()?;

        let db = client(path)?;
        let res = db.get_transaction_count(who, None)?;
        assert_eq!(res, nonce.into());
        Ok(())
    }

    #[test]
    fn test_get_storage_at() -> Result<()> {
        let mut rng = thread_rng();
        let who = Rand::rand(&mut rng);
        let key = Rand::rand(&mut rng);
        let val = Rand::rand(&mut rng);

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_storage(who, key, val)?;
        let path = w.close()?;

        let db = client(path)?;
        let read = db.get_storage_at(who, key, None)?;
        assert_eq!(read, val);
        Ok(())
    }

    #[test]
    fn test_get_block_number() -> Result<()> {
        let mut rng = thread_rng();
        let num = Rand::rand(&mut rng);
        let hash = keccak256(vec![0x10]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_head_header_hash(hash)?;
        w.put_header_number(hash, num)?;
        let path = w.close()?;

        let db = client(path)?;
        let res = db.get_block_number()?;
        assert_eq!(res, (*num).into());
        Ok(())
    }

    #[test]
    fn test_get_transaction() -> Result<()> {
        let mut rng = thread_rng();
        let txs = (0..).map(|_| MessageWithSignature::rand(&mut rng)).take(5).collect::<Vec<_>>();
        let tx_hashes = txs.iter().map(|tx| tx.hash());
        let block_num = Rand::rand(&mut rng);
        // let tx_hash = tx.hash();

        let block_body = BodyForStorage::rand(&mut rng);
        let base_tx_id = block_body.base_tx_id;
        let block_hash = H256::rand(&mut rng);

        let mut w = Writer::open(TMP_DIR.clone())?;
        // tx hash -> block number
        w.put_tx_lookup_entries(block_num, tx_hashes.clone())?;
        // block number -> block hash
        w.put_canonical_hash(block_hash, block_num)?;
        // (block number, block hash) -> body_for_storage
        w.put_body_for_storage(block_hash, block_num, block_body)?;
        // store transaction itself
        w.put_transactions(txs.clone(), *base_tx_id)?;
        let path = w.close()?;

        let db = client(path)?;
        for (i, hash) in tx_hashes.enumerate() {
            let res = db.get_transaction(hash)?;
            let expected = Some(MsgCast(&txs[i]).cast(block_num, block_hash, i));
            assert_eq!(res, expected);
        }
        Ok(())
    }
}
