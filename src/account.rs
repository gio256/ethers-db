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

impl akula::kv::traits::TableDecode for Account {
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
//TODO
impl akula::kv::traits::TableEncode for Account {
    type Encoded = Vec<u8>;
    fn encode(self) -> Self::Encoded {
        Self::Encoded::default()
    }
}

// https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/src/models/account.rs#L47
fn bytes_to_u64(buf: &[u8]) -> u64 {
    let mut decoded = [0u8; 8];
    for (i, b) in buf.iter().rev().enumerate() {
        decoded[i] = *b;
    }

    u64::from_le_bytes(decoded)
}

pub fn parse_u64_with_len(enc: &mut &[u8]) -> u64 {
    let len = enc.get_u8().into();
    let val = bytes_to_u64(&enc[..len]);
    enc.advance(len);
    val
}
