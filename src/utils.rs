use akula::{
    kv::mdbx::MdbxEnvironment,
    models::{Message, MessageWithSignature},
};
use anyhow::Result;
use ethers::types::H256;
use std::path::PathBuf;

pub fn open_db<E: mdbx::EnvironmentKind>(chaindata_dir: PathBuf) -> Result<MdbxEnvironment<E>> {
    MdbxEnvironment::<E>::open_ro(
        mdbx::Environment::new(),
        &chaindata_dir,
        // opening read-only, so the size of the DatabaseChat determines max_dbs,
        // but the contents are discarded
        akula::kv::tables::CHAINDATA_TABLES.clone(),
    )
}

// https://github.com/akula-bft/akula/blob/a9aed09b31bb41c89832149bcad7248f7fcd70ca/src/models/account.rs#L47
pub fn bytes_to_u64(buf: &[u8]) -> u64 {
    let mut decoded = [0u8; 8];
    for (i, b) in buf.iter().rev().enumerate() {
        decoded[i] = *b;
    }

    u64::from_le_bytes(decoded)
}

/// Converts akula block and message data into ethers data
pub struct Messenger<'a>(pub &'a MessageWithSignature);
impl<'a> Messenger<'a> {
    pub fn ethers_tx(
        &self,
        block_num: akula::models::BlockNumber,
        block_hash: H256,
        idx: usize,
    ) -> ethers::types::Transaction {
        ethers::types::Transaction {
            hash: self.0.hash(),
            nonce: self.0.nonce().into(),
            block_hash: Some(block_hash),
            block_number: Some(block_num.0.into()),
            transaction_index: Some(idx.into()),
            from: self.0.recover_sender().expect("bad sig"), //TODO: erigon has a separate table they merge instead
            to: self.0.action().into_address(),
            value: self.0.value().to_be_bytes().into(),
            gas_price: self.gas_price(),
            gas: self.0.gas_limit().into(),
            input: self.0.input().clone().into(),
            v: self.0.v().into(),
            r: self.0.r().to_fixed_bytes().into(),
            s: self.0.s().to_fixed_bytes().into(),
            transaction_type: self.tx_type(),
            access_list: self.access_list(),
            chain_id: self.0.chain_id().map(|id| id.0.into()),

            //TODO: should these be None for legacy txs?
            max_priority_fee_per_gas: Some(self.0.max_priority_fee_per_gas().to_be_bytes().into()),
            max_fee_per_gas: Some(self.0.max_fee_per_gas().to_be_bytes().into()),
        }
    }

    pub fn gas_price(&self) -> Option<ethers::types::U256> {
        match self.0.message {
            Message::Legacy { gas_price, .. } | Message::EIP2930 { gas_price, .. } => {
                Some(gas_price.to_be_bytes().into())
            }
            _ => None,
        }
    }

    fn tx_type(&self) -> Option<ethers::types::U64> {
        match self.0.message {
            Message::EIP2930 { .. } => Some(1.into()),
            Message::EIP1559 { .. } => Some(2.into()),
            _ => None,
        }
    }

    fn access_list(&self) -> Option<ethers::types::transaction::eip2930::AccessList> {
        match &self.0.message {
            Message::EIP2930 { access_list, .. } | Message::EIP1559 { access_list, .. } => Some(
                access_list
                    .iter()
                    .map(|it| ethers::types::transaction::eip2930::AccessListItem {
                        address: it.address,
                        storage_keys: it.slots.clone(),
                    })
                    .collect::<Vec<_>>()
                    .into(),
            ),
            _ => None,
        }
    }
}
