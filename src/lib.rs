pub mod middleware;

mod account;
mod db;
mod storage;
mod tables;
mod utils;

#[cfg(test)]
mod ffi;

#[cfg(test)]
mod tests {
    use super::{middleware::DbMiddleware, utils::*};
    use anyhow::{format_err, Result};
    use ethers::providers::{Middleware, MockProvider, Provider};
    use mdbx::NoWriteMap;
    use once_cell::sync::Lazy;
    use std::{path::PathBuf, sync::Arc};

    const TMP_DIR_ENV_LABEL: &str = "CHAINDATA_TMP_DIR";
    const LINK_TEST_BIN: &str = "LINK_TEST_BIN";

    pub(crate) static TMP_DIR: Lazy<PathBuf> = Lazy::new(|| tmp_dir().unwrap());

    pub(crate) fn get_db(path: PathBuf) -> Result<DbMiddleware<impl Middleware, NoWriteMap>> {
        let db = Arc::new(open_db(path)?);
        let provider = Provider::new(MockProvider::new());
        Ok(DbMiddleware::new(provider, db))
    }

    fn tmp_dir() -> Result<PathBuf> {
        let path = std::env::var(TMP_DIR_ENV_LABEL).map_err(|e| {
            if std::env::var(LINK_TEST_BIN).is_err() {
                format_err!("Err: {}\nExport {} to run the tests.", e, LINK_TEST_BIN)
            } else {
                format_err!(
                    "Err: {}\nCan't get {}. This is likely a problem with the build script.",
                    e,
                    TMP_DIR_ENV_LABEL
                )
            }
        })?;
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod rand {
    use akula::models::{
        Address, ChainId, Message, MessageSignature, MessageWithSender, MessageWithSignature,
        TransactionAction, H256, U256,
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
    rand!(u64);
    rand!([u128; 2]);
    rand!(Address);
    rand_unit!(U256);
    rand_unit!(ChainId);
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
}
