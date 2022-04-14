#[cfg(test)]
pub mod ffi {
    use crate::account::Account;
    use ethers::types::{Address, U256, H256};
    use akula::models::{RlpAccount};
    use anyhow::Result;
    use fastrlp::*;
    use libc::c_void;

    const KECCAK_LENGTH: u64 = 32;

    #[repr(C)]
    struct GoSlice<'a> {
        ptr: *mut c_void,
        len: u64,
        cap: u64,
        _tick: std::marker::PhantomData<&'a usize>,
    }

    impl<'a> From<&'a mut [u8]> for GoSlice<'a> {
        fn from(src: &mut [u8]) -> Self {
            Self {
                ptr: src.as_mut_ptr() as *mut c_void,
                len: src.len() as u64,
                cap: src.len() as u64,
                _tick: std::marker::PhantomData,
            }
        }
    }
    impl<'a> From<&'a mut Address> for GoSlice<'a> {
        fn from(src: &'a mut Address) -> Self {
            Self {
                ptr: src.as_mut_ptr() as *mut c_void,
                len: Address::len_bytes() as u64,
                cap: Address::len_bytes() as u64,
                _tick: std::marker::PhantomData,
            }
        }
    }
    impl<'a> From<&'a mut U256> for GoSlice<'a> {
        fn from(src: &'a mut U256) -> Self {
            Self {
                ptr: src.0.as_mut_ptr() as *mut c_void,
                len: KECCAK_LENGTH,
                cap: KECCAK_LENGTH,
                _tick: std::marker::PhantomData,
            }
        }
    }
    impl<'a> From<&'a mut H256> for GoSlice<'a> {
        fn from(src: &'a mut H256) -> Self {
            Self {
                ptr: src.0.as_mut_ptr() as *mut c_void,
                len: H256::len_bytes() as u64,
                cap: H256::len_bytes() as u64,
                _tick: std::marker::PhantomData,
            }
        }
    }

    impl From<Account> for RlpAccount {
        fn from(src: Account) -> Self {
            let mut bal = [0; 32];
            src.balance.to_big_endian(&mut bal);
            RlpAccount {
                nonce: src.nonce,
                balance: akula::models::U256::from_be_bytes(bal),
                storage_root: Default::default(),
                code_hash: src.codehash,
            }
        }
    }

    extern "C" {
        fn PutAccount(address: GoSlice, rlpAccount: GoSlice, incarnation: u64);
        fn PutStorage(address: GoSlice, key: GoSlice, val: GoSlice);
        fn DbInit();
    }

    pub fn db_init() {
        unsafe { DbInit() }
    }

    pub fn put_account(mut who: Address, acct: Account) -> Result<()> {
        let rlp_acct: RlpAccount = acct.into();
        let mut buf = vec![];
        rlp_acct.encode(&mut buf);

        unsafe { PutAccount((&mut who).into(), (&mut buf[..]).into(), acct.incarnation) };
        Ok(())
    }

    pub fn put_storage(mut who: Address, mut key: H256, mut val: H256) -> Result<()> {
        unsafe { PutStorage((&mut who).into(), (&mut key).into(), (&mut val).into()) };
        Ok(())
    }
}
