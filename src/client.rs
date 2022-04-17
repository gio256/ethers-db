use akula::kv::{mdbx::MdbxEnvironment, tables as ak_tables};
use anyhow::{format_err, Result};
use ethers::core::types::{
    Address, Block, BlockId, BlockNumber as EthersBlockNumber, TxHash, H256, U256, U64,
};
use mdbx::{EnvironmentKind, TransactionKind};
use std::path::PathBuf;

use crate::reader::Reader;
use crate::utils::{open_db, BlockCast, MsgCast};

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

        let (msg, idx) = dbtx
            .try_stream_transactions(*body.base_tx_id, body.tx_amount.try_into()?)?
            .zip(0..)
            .find(|(msg, _i)| msg.hash() == hash)
            .ok_or_else(|| format_err!("No transaction hash {} in block {}", hash, block_num))?;

        Ok(Some(MsgCast::new(&msg).cast(block_num, block_hash, idx)))
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
        let header_key = get_header_key(&mut dbtx, block_hash_or_number)?;
        let body = dbtx.read_body_for_storage(header_key)?;
        Ok(body.uncles.len().into())
    }

    pub fn get_uncle<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
        idx: U64,
    ) -> Result<Option<Block<H256>>> {
        let mut dbtx = self.reader()?;
        let header_key = get_header_key(&mut dbtx, block_hash_or_number)?;
        let body = dbtx.read_body_for_storage(header_key)?;
        let idx = idx.as_usize();
        if idx < body.uncles.len() {
            self.get_block(body.uncles[idx].number.0)
        } else {
            Ok(None)
        }
    }

    //TODO: should also look for non-canonical blocks?
    // https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/bin/akula.rs#L266
    pub fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>> {
        let mut dbtx = self.reader()?;

        let header_key = get_header_key(&mut dbtx, block_hash_or_number)?;
        let (block_num, block_hash) = header_key;

        let header = dbtx.read_header(header_key)?;
        let body = dbtx.read_body_for_storage(header_key)?;

        let tx_amt: usize = body.tx_amount.try_into()?;
        let txs = dbtx
            .stream_transactions(body.base_tx_id.0)?
            .map(|msg| Ok(msg?.hash()))
            .take(body.tx_amount.try_into()?)
            .collect::<Result<Vec<_>>>()?;

        if txs.len() != tx_amt {
            return Err(format_err!(
                "Failed to get some txs in block {}. Expected: {}. Got {}",
                block_num,
                tx_amt,
                txs.len()
            ));
        }

        let ommer_hashes = body
            .uncles
            .iter()
            .map(|header| dbtx.read_canonical_hash(header.number))
            .collect::<Result<Vec<_>>>()?;

        let block = BlockCast(&header).cast(txs, block_num, block_hash, ommer_hashes);
        Ok(Some(block))
    }

    pub fn get_block_with_txs<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<ethers::types::Transaction>>> {
        let mut dbtx = self.reader()?;

        let header_key = get_header_key(&mut dbtx, block_hash_or_number)?;
        let (block_num, block_hash) = header_key;

        let header = dbtx.read_header(header_key)?;
        let body = dbtx.read_body_for_storage(header_key)?;

        // try_stream_transactions so we can cast the txs as we read them
        let tx_amt = body.tx_amount.try_into()?;
        let txs = dbtx
            .try_stream_transactions(body.base_tx_id.0, tx_amt)?
            .scan(0_usize, |idx, msg| {
                let tx = MsgCast::new(&msg).cast(block_num, block_hash, *idx);
                *idx += 1;
                Some(tx)
            })
            .collect::<Vec<_>>();

        // If we failed to read any txs, they were discarded, so make sure we got them all
        if txs.len() != tx_amt {
            return Err(format_err!(
                "Failed to get some txs in block {}. Expected: {}. Got {}",
                block_num,
                tx_amt,
                txs.len()
            )
            .into());
        }

        let ommer_hashes = body
            .uncles
            .iter()
            .map(|header| dbtx.read_canonical_hash(header.number))
            .collect::<Result<Vec<_>>>()?;

        let block = crate::utils::BlockCast(&header).cast(txs, block_num, block_hash, ommer_hashes);
        Ok(Some(block))
    }
}

/// Returns the (block number, block hash) key used to identify a block in the db
pub fn get_header_key<T: Into<BlockId> + Send + Sync, TX: TransactionKind, E: EnvironmentKind>(
    dbtx: &mut Reader<'_, TX, E>,
    id: T,
) -> Result<ak_tables::HeaderKey> {
    let (num, hash) = match id.into() {
        BlockId::Hash(hash) => {
            let num = dbtx.read_header_number(hash)?.0.into();
            (num, hash)
        }
        BlockId::Number(id) => match id {
            EthersBlockNumber::Number(n) => (n, dbtx.read_canonical_hash(n.as_u64().into())?),
            //TODO: check this https://github.com/ledgerwatch/erigon/blob/156da607e7495d709c141aec40f66a2556d35dc0/cmd/rpcdaemon/commands/rpc_block.go#L30
            EthersBlockNumber::Latest | EthersBlockNumber::Pending => {
                let hash = dbtx.read_head_header_hash()?;
                let num = dbtx.read_header_number(hash)?;
                (num.0.into(), hash)
            }
            EthersBlockNumber::Earliest => (0.into(), dbtx.read_canonical_hash(0.into())?),
        },
    };
    Ok((num.as_u64().into(), hash))
}

#[cfg(test)]
mod tests {
    use akula::models::{Block, BodyForStorage, MessageWithSignature, H256};
    use anyhow::Result;
    use ethers::{core::types::Address, utils::keccak256};
    use std::path::PathBuf;

    use super::Client;
    use crate::{
        account::Account,
        ffi::writer::Writer,
        rand::{rand_vec, Rand},
        tests::TMP_DIR,
        utils::{BlockCast, MsgCast},
    };
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
        let txs = (0..)
            .map(|_| MessageWithSignature::rand(&mut rng))
            .take(5)
            .collect::<Vec<_>>();
        let tx_hashes = txs.iter().map(|tx| tx.hash());
        let block_num = Rand::rand(&mut rng);

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
        // store transactions themselves
        w.put_transactions(txs.clone(), *base_tx_id)?;
        let path = w.close()?;

        let db = client(path)?;
        for (i, hash) in tx_hashes.enumerate() {
            let res = db.get_transaction(hash)?;
            let expected = Some(MsgCast::new(&txs[i]).cast(block_num, block_hash, i));
            assert_eq!(res, expected);
        }
        Ok(())
    }

    #[test]
    fn test_get_block() -> Result<()> {
        let mut rng = thread_rng();
        let mut block = Block::rand(&mut rng);
        block.transactions = rand_vec(&mut rng, 5);
        block.ommers = rand_vec(&mut rng, 5);
        let body_for_storage = BodyForStorage {
            base_tx_id: Rand::rand(&mut rng),
            tx_amount: (block.transactions.len() + 2).try_into()?,
            uncles: block.ommers.clone(),
        };
        let base_tx_id = *body_for_storage.base_tx_id;
        let block_hash = block.header.hash();
        let block_num = block.header.number;

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_header_number(block_hash, block_num)?;
        w.put_header(block.header.clone())?;
        w.put_body_for_storage(block_hash, block.header.number, body_for_storage)?;
        w.put_transactions(block.transactions.clone(), base_tx_id)?;

        // write ommer hashes to db and save them for checking the result
        let mut ommer_hashes = vec![];
        for ommer in block.ommers.clone() {
            ommer_hashes.push(ommer.hash());
            w.put_canonical_hash(ommer.hash(), ommer.number)?;
        }

        let path = w.close()?;
        let db = client(path)?;

        // test get_block_with_txs
        let res = db.get_block_with_txs(block_hash)?;
        let expected_txs = block
            .transactions
            .iter()
            .zip(0..)
            .map(|(tx, i)| MsgCast::new(tx).cast(block_num, block_hash, i))
            .collect();
        let expected = BlockCast(&block.header).cast(
            expected_txs,
            block_num,
            block_hash,
            ommer_hashes.clone(),
        );
        assert_eq!(res, Some(expected));

        // test get_block
        let res = db.get_block(block_hash)?;
        let expected_txs = block.transactions.iter().map(|tx| tx.hash()).collect();
        let expected =
            BlockCast(&block.header).cast(expected_txs, block_num, block_hash, ommer_hashes);
        assert_eq!(res, Some(expected));
        Ok(())
    }

    #[test]
    fn test_get_header_key() -> Result<()> {
        Ok(())
    }
}
