use crate::models::{Account, StorageBucket};
use akula::decl_table;
use ethers::types::{Address, H256};

// pub use crate::models::Storage;

decl_table!(LastHeader => Vec<u8> => H256);
decl_table!(LastBlock => Vec<u8> => H256);
decl_table!(IncarnationMap => Address => u64);
// Erigon's TxLookup table
decl_table!(BlockTransactionLookup => H256 => akula::models::U256);
decl_table!(PlainState => Address => Account);

// Custom table for account storage because it overlaps with PlainState
#[derive(Clone, Copy, Debug, Default)]
pub struct Storage;

impl akula::kv::Table for Storage {
    type Key = StorageBucket;
    type SeekKey = StorageBucket;
    type Value = (H256, akula::models::U256);

    fn db_name(&self) -> string::String<bytes::Bytes> {
        string::String::from_str("PlainState")
    }
}
impl akula::kv::DupSort for Storage {
    type SeekBothKey = H256;
}
