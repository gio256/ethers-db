use akula::kv::traits as ak_traits;
use anyhow::format_err;
use bytes::Buf;
use ethers::types::{H256, U256};

const KECCAK_LENGTH: usize = H256::len_bytes();

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Account {
    pub nonce: u64,
    pub incarnation: u64,
    pub balance: U256,
    pub codehash: H256, // hash of the bytecode
}

impl ak_traits::TableDecode for Account {
    fn decode(mut enc: &[u8]) -> anyhow::Result<Self> {
        let mut acct = Self::default();

        if enc.is_empty() {
            return Ok(acct);
        }

        let fieldset = enc.get_u8();

        // has nonce
        if fieldset & 1 > 0 {
            acct.nonce = parse_u64_with_len(&mut enc);
        }

        // has balance
        if fieldset & 2 > 0 {
            let bal_len = enc.get_u8();
            acct.balance = enc[..bal_len.into()].into();
            enc.advance(bal_len.into());
        }

        // has incarnation
        if fieldset & 4 > 0 {
            acct.incarnation = parse_u64_with_len(&mut enc);
        }

        // has codehash
        if fieldset & 8 > 0 {
            let len: usize = enc.get_u8().into();
            if len != KECCAK_LENGTH {
                return Err(format_err!(
                    "codehash should be {} bytes long. Got {} instead",
                    KECCAK_LENGTH,
                    len
                ));
            }
            acct.codehash = H256::from_slice(&enc[..KECCAK_LENGTH]);
            enc.advance(KECCAK_LENGTH)
        }

        // TODO: erigon docs mention storage hash field, code seems to disagree
        if enc.remaining() > 0 {
            return Err(format_err!("unexpected account field"));
        }

        Ok(acct)
    }
}
//TODO: dummy impl as we only need to decode for now, but need the trait bound
impl ak_traits::TableEncode for Account {
    type Encoded = Vec<u8>;
    fn encode(self) -> Self::Encoded {
        Self::Encoded::default()
    }
}

pub fn parse_u64_with_len(enc: &mut &[u8]) -> u64 {
    let len = enc.get_u8().into();
    let val = crate::utils::bytes_to_u64(&enc[..len]);
    enc.advance(len);
    val
}

impl Account {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }
    pub fn incarnation(mut self, inc: u64) -> Self {
        self.incarnation = inc;
        self
    }
    pub fn balance(mut self, bal: U256) -> Self {
        self.balance = bal;
        self
    }
    pub fn codehash(mut self, hash: H256) -> Self {
        self.codehash = hash;
        self
    }
}
