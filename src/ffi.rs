const KECCAK_LENGTH: u64 = 32;

#[cfg(test)]
pub mod ffi {
    use super::*;
    use crate::account::Account;
    use akula::models::RlpAccount;
    use anyhow::{bail, Result};
    use ethers::types::{Address, H256, U256};
    use fastrlp::*;
    use libc::{c_void, uintptr_t};
    use std::{
        ffi::CString,
        mem,
        os::raw::c_char,
        path::{Path, PathBuf},
    };

    pub struct Writer {
        path: PathBuf,
        db_ptr: GoPtr,
    }
    impl Writer {
        pub fn open<P: AsRef<Path>>(p: P) -> Result<Self> {
            // generate db path and open db
            let path = tempfile::Builder::new().tempdir_in(p)?.into_path();
            let c_path = CString::new(path.to_str().unwrap()).unwrap();
            let go_path = GoString {
                a: c_path.as_ptr(),
                b: c_path.as_bytes().len() as i64,
            };
            let GoTuple { a: exit, b: db_ptr } = unsafe { MdbxOpen(go_path) };

            if exit < 1 {
                bail!("MdbxOpen failed with exit code {}", exit)
            }
            Ok(Self {
                path: path.to_path_buf(),
                db_ptr,
            })
        }
        pub fn put_account(&mut self, mut who: Address, acct: Account) -> Result<()> {
            let rlp_acct: RlpAccount = acct.into();
            let mut buf = vec![];
            rlp_acct.encode(&mut buf);

            let exit = unsafe {
                PutAccount(
                    self.db_ptr,
                    (&mut who).into(),
                    (&mut buf[..]).into(),
                    acct.incarnation,
                )
            };
            if exit < 1 {
                bail!("MdbxOpen failed with exit code {}", exit)
            }
            Ok(())
        }

        pub fn put_storage(
            &mut self,
            mut who: Address,
            mut key: H256,
            mut val: H256,
        ) -> Result<()> {
            let exit = unsafe {
                PutStorage(
                    self.db_ptr,
                    (&mut who).into(),
                    (&mut key).into(),
                    (&mut val).into(),
                )
            };
            if exit < 1 {
                bail!("MdbxOpen failed with exit code {}", exit)
            }
            Ok(())
        }

        pub fn close(mut self) -> Result<PathBuf> {
            unsafe { MdbxClose(self.db_ptr) }
            // consume without running drop()
            let path = mem::replace(&mut self.path, PathBuf::new());
            mem::forget(self);
            Ok(path)
        }
    }
    impl Drop for Writer {
        fn drop(&mut self) {
            unsafe { MdbxClose(self.db_ptr) }
        }
    }

    type GoPtr = uintptr_t;
    type GoExit = i64;

    #[repr(C)]
    struct GoSlice<'a> {
        ptr: *mut c_void,
        len: u64,
        cap: u64,
        _tick: std::marker::PhantomData<&'a usize>,
    }

    #[repr(C)]
    struct GoString {
        a: *const c_char,
        b: i64,
    }

    #[repr(C)]
    struct GoTuple<A, B> {
        a: A,
        b: B,
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
        fn MdbxOpen(path: GoString) -> GoTuple<GoExit, GoPtr>;
        fn MdbxClose(db: GoPtr);
        fn PutStorage(db: GoPtr, address: GoSlice, key: GoSlice, val: GoSlice) -> GoExit;
        fn PutAccount(
            ptr: GoPtr,
            address: GoSlice,
            rlpAccount: GoSlice,
            incarnation: u64,
        ) -> GoExit;
    }
}
