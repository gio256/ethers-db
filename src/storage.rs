use ethers::types::{Address, H256};

const ADDRESS_LENGTH: usize = Address::len_bytes();
const U64_LENGTH: usize = std::mem::size_of::<u64>();

// Custom table for storage because it overlaps with PlainState
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct StorageBucket {
    pub address: Address,
    pub incarnation: u64,
}
impl StorageBucket {
    pub fn new(address: Address, incarnation: u64) -> Self {
        Self {
            address,
            incarnation,
        }
    }
}

impl akula::kv::TableEncode for StorageBucket {
    type Encoded = [u8; ADDRESS_LENGTH + U64_LENGTH];

    fn encode(self) -> Self::Encoded {
        let mut out = [0; ADDRESS_LENGTH + U64_LENGTH];
        out[..ADDRESS_LENGTH].copy_from_slice(&self.address.encode());
        out[ADDRESS_LENGTH..].copy_from_slice(&self.incarnation.encode());
        out
    }
}
//TODO: dummy impl as we only need to encode for now, but need the trait bound
impl akula::kv::TableDecode for StorageBucket {
    fn decode(_enc: &[u8]) -> anyhow::Result<Self> {
        Ok(Default::default())
    }
}

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
