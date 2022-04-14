#[cfg(test)]
pub mod ffi {
    use crate::account::Account;
    use akula::models::{Address, RlpAccount};
    use anyhow::Result;
    use fastrlp::*;
    use libc::c_void;

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
    }

    pub fn put_account(mut who: Address, acct: Account) -> Result<()> {
        let rlp_acct: RlpAccount = acct.into();
        let mut buf = vec![];
        rlp_acct.encode(&mut buf);

        unsafe { PutAccount((&mut who).into(), (&mut buf[..]).into(), acct.incarnation) };

        Ok(())
    }
}
