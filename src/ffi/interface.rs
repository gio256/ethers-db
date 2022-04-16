use crate::account::Account;
use akula::models::RlpAccount;
use ethers::types::{Address, H256, U256};
use libc::{c_void, uintptr_t};
use std::{ffi::CStr, fmt::Debug, marker::PhantomData, os::raw::c_char};

const KECCAK_LENGTH: u64 = 32;

extern "C" {
    pub(crate) fn MdbxOpen(path: GoPath) -> GoTuple<GoExit, GoPtr>;
    pub(crate) fn MdbxClose(db: GoPtr);
    pub(crate) fn PutHeadHeaderHash(db: GoPtr, hash: GoU256) -> GoExit;
    pub(crate) fn PutHeaderNumber(db: GoPtr, hash: GoU256, num: u64) -> GoExit;
    pub(crate) fn PutCanonicalHash(db: GoPtr, hash: GoU256, num: u64) -> GoExit;
    pub(crate) fn PutStorage(db: GoPtr, address: GoAddress, key: GoU256, val: GoU256) -> GoExit;
    #[allow(unused)]
    pub(crate) fn PutRawTransactions(db: GoPtr, txs: GoSlice, baseId: u64) -> GoExit;
    pub(crate) fn PutTransactions(db: GoPtr, txs: GoSlice, baseId: u64) -> GoExit;
    pub(crate) fn PutAccount(
        ptr: GoPtr,
        address: GoAddress,
        rlpAccount: GoRlp,
        incarnation: u64,
    ) -> GoExit;
}

#[repr(transparent)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoRlp<'a>(pub GoSlice<'a>);

#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoTuple<A, B> {
    pub a: A,
    pub b: B,
}

#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoSlice<'a> {
    ptr: *mut c_void,
    len: u64,
    cap: u64,
    _tick: std::marker::PhantomData<&'a ()>,
}

impl<'a, T> From<&'a mut [T]> for GoSlice<'a> {
    fn from(src: &'a mut [T]) -> Self {
        Self {
            ptr: src.as_mut_ptr() as *mut c_void,
            len: src.len() as u64,
            cap: src.len() as u64,
            _tick: std::marker::PhantomData,
        }
    }
}
impl<'a> From<&'a mut bytes::BytesMut> for GoSlice<'a> {
    fn from(src: &'a mut bytes::BytesMut) -> Self {
        Self {
            ptr: src[..].as_mut_ptr() as *mut c_void,
            len: src.len() as u64,
            cap: src.len() as u64,
            _tick: std::marker::PhantomData,
        }
    }
}

#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoPath<'s> {
    ptr: *const c_char,
    len: i64,
    _tick: PhantomData<&'s ()>,
}

impl<'s> From<&'s str> for GoPath<'s> {
    // Panics if src is not null-terminated
    fn from(src: &'s str) -> Self {
        let cstr = CStr::from_bytes_with_nul(src.as_bytes()).expect("must null terminate cstring");
        Self {
            ptr: cstr.as_ptr(),
            len: cstr.to_bytes().len() as i64,
            _tick: PhantomData,
        }
    }
}

pub fn null_term(s: &str) -> String {
    let mut s = String::from(s);
    if s.bytes().last() != Some(0) {
        s.push('\0')
    }
    s
}

// -- Newtypes for a little type safety

// No methods. Don't touch it!
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GoPtr(uintptr_t);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GoExit(i64);

impl GoExit {
    pub fn is_err(&self) -> bool {
        self.0 < 1
    }
    pub fn is_ok(&self) -> bool {
        !self.is_err()
    }
    pub fn ok_or_fmt<E: Debug>(&self, err_msg: E) -> anyhow::Result<i64> {
        if self.is_ok() {
            Ok(self.0)
        } else {
            anyhow::bail!("{:?} errored with exit code {}", err_msg, self.0)
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoAddress<'a>(GoSlice<'a>);

impl<'a> From<&'a mut Address> for GoAddress<'a> {
    fn from(src: &'a mut Address) -> Self {
        Self(GoSlice {
            ptr: src.as_mut_ptr() as *mut c_void,
            len: Address::len_bytes() as u64,
            cap: Address::len_bytes() as u64,
            _tick: std::marker::PhantomData,
        })
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoU256<'a>(GoSlice<'a>);

impl<'a> From<&'a mut H256> for GoU256<'a> {
    fn from(src: &'a mut H256) -> Self {
        Self(GoSlice {
            ptr: src.0.as_mut_ptr() as *mut c_void,
            len: H256::len_bytes() as u64,
            cap: H256::len_bytes() as u64,
            _tick: std::marker::PhantomData,
        })
    }
}

impl<'a> From<&'a mut U256> for GoU256<'a> {
    fn from(src: &'a mut U256) -> Self {
        Self(GoSlice {
            ptr: src.0.as_mut_ptr() as *mut c_void,
            len: KECCAK_LENGTH,
            cap: KECCAK_LENGTH,
            _tick: std::marker::PhantomData,
        })
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
