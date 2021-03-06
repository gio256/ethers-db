use akula::models::{
    Address, Block, BlockHeader, BodyForStorage, Message, MessageSignature, MessageWithSender,
    MessageWithSignature, TransactionAction, H256,
};
use ethers::core::k256::{
    ecdsa::{recoverable::Signature, signature::Signer, SigningKey},
    elliptic_curve::FieldBytes,
    Secp256k1,
};
use rand::{rngs::ThreadRng, Rng, RngCore};

pub trait Rand {
    fn rand(rng: &mut ThreadRng) -> Self;
}

macro_rules! rand {
    ($t:ty) => {
        impl Rand for $t {
            fn rand(rng: &mut ThreadRng) -> Self {
                rng.gen::<Self>()
            }
        }
    };
}
macro_rules! rand_unit {
    ($t:ty) => {
        impl Rand for $t {
            fn rand(rng: &mut ThreadRng) -> Self {
                Self(Rand::rand(rng))
            }
        }
    };
}
rand!(u32);
rand!(u64);
rand!([u8; 8]);
rand!([u8; 32]);
rand!([u128; 2]);
rand!(akula::models::Address);
rand_unit!(akula::models::U256);
rand_unit!(akula::models::H256);
rand_unit!(akula::models::BlockNumber);
rand_unit!(akula::models::TxIndex);
rand_unit!(akula::models::H64);
rand_unit!(akula::models::Bloom);
impl Rand for [u8; 256] {
    fn rand(rng: &mut ThreadRng) -> Self {
        let mut buf = [0; 256];
        rng.fill(&mut buf);
        buf
    }
}
impl Rand for akula::models::ChainId {
    fn rand(rng: &mut ThreadRng) -> Self {
        // prevent overflow when finding v for eip-155 (https://eips.ethereum.org/EIPS/eip-155)
        // https://github.com/gio256/akula/blob/d2241fe03b0d0ada8743af625acbbe812e62f597/src/models/transaction.rs#L131
        let max = u64::MAX / 2 - 35;
        Self(rng.gen_range(0..max))
    }
}
impl Rand for TransactionAction {
    fn rand(rng: &mut ThreadRng) -> Self {
        if rng.gen::<bool>() {
            Self::Call(rng.gen::<Address>())
        } else {
            Self::Create
        }
    }
}
impl Rand for bytes::Bytes {
    fn rand(rng: &mut ThreadRng) -> Self {
        let cap = rng.gen::<u8>() as usize;
        let mut data = vec![0; cap];
        rng.fill_bytes(&mut data);
        data.into()
    }
}
impl<T> Rand for Option<T>
where
    T: Rand,
{
    fn rand(rng: &mut ThreadRng) -> Self {
        if rng.gen::<bool>() {
            Some(Rand::rand(rng))
        } else {
            None
        }
    }
}
pub fn rand_legacy(rng: &mut ThreadRng) -> Message {
    Message::Legacy {
        chain_id: Rand::rand(rng),
        nonce: Rand::rand(rng),
        gas_price: Rand::rand(rng),
        gas_limit: Rand::rand(rng),
        action: Rand::rand(rng),
        value: Rand::rand(rng),
        input: Rand::rand(rng),
    }
}
pub fn rand_1559(rng: &mut ThreadRng) -> Message {
    Message::EIP1559 {
        chain_id: Rand::rand(rng),
        nonce: Rand::rand(rng),
        max_priority_fee_per_gas: Rand::rand(rng),
        max_fee_per_gas: Rand::rand(rng),
        gas_limit: Rand::rand(rng),
        action: Rand::rand(rng),
        value: Rand::rand(rng),
        input: Rand::rand(rng),
        access_list: Default::default(),
    }
}
pub fn rand_2930(rng: &mut ThreadRng) -> Message {
    Message::EIP2930 {
        chain_id: Rand::rand(rng),
        nonce: Rand::rand(rng),
        gas_price: Rand::rand(rng),
        gas_limit: Rand::rand(rng),
        action: Rand::rand(rng),
        value: Rand::rand(rng),
        input: Rand::rand(rng),
        access_list: Default::default(),
    }
}

impl Rand for Message {
    fn rand(rng: &mut ThreadRng) -> Self {
        let n = rng.gen_range(0..3);
        if n == 0 {
            return rand_legacy(rng);
        }
        if n == 1 {
            return rand_1559(rng);
        }
        rand_2930(rng)
    }
}
impl Rand for MessageWithSender {
    fn rand(rng: &mut ThreadRng) -> Self {
        Self {
            message: Rand::rand(rng),
            sender: Rand::rand(rng),
        }
    }
}
impl Rand for MessageWithSignature {
    fn rand(rng: &mut ThreadRng) -> Self {
        let msg = Message::rand(rng);
        let key = SigningKey::random(rng);
        let sig = sign(key, msg.hash().as_bytes());
        Self {
            message: msg,
            signature: sig,
        }
    }
}
pub fn sign(key: SigningKey, msg: &[u8]) -> MessageSignature {
    let rsig: Signature = key.sign(msg);
    let v = match rsig.recovery_id().into() {
        0 => false,
        1 => true,
        _ => panic!("Can't normalize ecdsa recovery id"),
    };
    let r: FieldBytes<Secp256k1> = rsig.r().into();
    let s: FieldBytes<Secp256k1> = rsig.s().into();
    MessageSignature::new(
        v,
        H256::from_slice(r.as_slice()),
        H256::from_slice(s.as_slice()),
    )
    .expect("Generated a bad signature")
}

impl Rand for BodyForStorage {
    fn rand(rng: &mut ThreadRng) -> Self {
        Self {
            base_tx_id: Rand::rand(rng),
            tx_amount: u32::rand(rng).into(), // erigon stores TxAmount as uint32
            uncles: Default::default(),
        }
    }
}

impl Rand for BlockHeader {
    fn rand(rng: &mut ThreadRng) -> Self {
        Self {
            parent_hash: Rand::rand(rng),
            ommers_hash: Rand::rand(rng),
            beneficiary: Rand::rand(rng),
            state_root: Rand::rand(rng),
            transactions_root: Rand::rand(rng),
            receipts_root: Rand::rand(rng),
            logs_bloom: Rand::rand(rng),
            difficulty: Rand::rand(rng),
            number: Rand::rand(rng),
            gas_limit: Rand::rand(rng),
            gas_used: Rand::rand(rng),
            timestamp: Rand::rand(rng),
            extra_data: Rand::rand(rng),
            mix_hash: Rand::rand(rng),
            nonce: Rand::rand(rng),
            base_fee_per_gas: Rand::rand(rng),
        }
    }
}

impl Rand for Block {
    fn rand(rng: &mut ThreadRng) -> Self {
        Self {
            header: Rand::rand(rng),
            transactions: Default::default(),
            ommers: Default::default(),
        }
    }
}

pub fn rand_vec<T: Rand>(rng: &mut ThreadRng, n: usize) -> Vec<T> {
    (0..).map(|_| Rand::rand(rng)).take(n).collect()
}
