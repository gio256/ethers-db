use crate::account::Account;
use akula::decl_table;
use ethers::types::{Address, H256};

pub use crate::storage::Storage;

decl_table!(LastHeader => Vec<u8> => H256);
decl_table!(LastBlock => Vec<u8> => H256);
decl_table!(IncarnationMap => Address => u64);
decl_table!(BlockTransactionLookup => H256 => akula::models::U256);
decl_table!(PlainState => Address => Account);
