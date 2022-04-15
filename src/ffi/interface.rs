use crate::account::Account;
use akula::models::RlpAccount;
use ethers::types::{Address, H256, U256};
use libc::{c_void, uintptr_t};
use std::{ffi::CString, os::raw::c_char};

const KECCAK_LENGTH: u64 = 32;

extern "C" {
    pub(crate) fn MdbxOpen(path: GoString) -> GoTuple<GoExit, GoPtr>;
    pub(crate) fn MdbxClose(db: GoPtr);
    pub(crate) fn PutStorage(db: GoPtr, address: GoSlice, key: GoSlice, val: GoSlice) -> GoExit;
    pub(crate) fn PutAccount(
        ptr: GoPtr,
        address: GoSlice,
        rlpAccount: GoSlice,
        incarnation: u64,
    ) -> GoExit;
}

pub(crate) type GoPtr = uintptr_t;
pub(crate) type GoExit = i64;

#[repr(C)]
pub(crate) struct GoTuple<A, B> {
    pub a: A,
    pub b: B,
}

#[repr(C)]
pub(crate) struct GoString {
    ptr: *const c_char,
    len: i64,
}

impl From<&CString> for GoString {
    fn from(src: &CString) -> Self {
        Self {
            ptr: src.as_ptr(),
            len: src.as_bytes().len() as i64,
        }
    }
}

#[repr(C)]
pub(crate) struct GoSlice<'a> {
    ptr: *mut c_void,
    len: u64,
    cap: u64,
    _tick: std::marker::PhantomData<&'a ()>,
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
