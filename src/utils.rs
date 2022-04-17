use akula::{
    kv::mdbx::MdbxEnvironment,
    models::{Address, BlockHeader, Message, MessageWithSignature},
};
use anyhow::Result;
use ethers::types::H256;
use std::path::PathBuf;

pub fn open_db<E: mdbx::EnvironmentKind>(chaindata_dir: PathBuf) -> Result<MdbxEnvironment<E>> {
    MdbxEnvironment::<E>::open_ro(
        mdbx::Environment::new(),
        &chaindata_dir,
        // opening read-only, so the size of the DatabaseChart determines max_dbs,
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

/// Converts akula block and message data into ethers transaction data
pub struct MsgCast<'a> {
    pub msg: &'a MessageWithSignature,
    pub src: Option<Address>,
}
impl<'a> MsgCast<'a> {
    pub fn new(msg: &'a MessageWithSignature) -> Self {
        Self { msg, src: None }
    }

    pub fn maybe_signer(&mut self, src: Address) -> &mut Self {
        if src != Default::default() {
            self.src = Some(src)
        }
        self
    }

    pub fn cast(
        &self,
        block_num: akula::models::BlockNumber,
        block_hash: H256,
        idx: usize,
    ) -> ethers::types::Transaction {
        let from = if let Some(src) = self.src {
            src
        } else {
            self.msg.recover_sender().expect("bad sig")
        };
        ethers::types::Transaction {
            hash: self.msg.hash(),
            nonce: self.msg.nonce().into(),
            block_hash: Some(block_hash),
            block_number: Some((*block_num).into()),
            transaction_index: Some(idx.into()),
            from,
            to: self.msg.action().into_address(),
            value: self.msg.value().to_be_bytes().into(),
            gas_price: self.gas_price(),
            gas: self.msg.gas_limit().into(),
            input: self.msg.input().clone().into(),
            v: self.msg.v().into(),
            r: self.msg.r().to_fixed_bytes().into(),
            s: self.msg.s().to_fixed_bytes().into(),
            transaction_type: self.tx_type(),
            access_list: self.access_list(),
            chain_id: self.msg.chain_id().map(|id| (*id).into()),

            //TODO: should these be None for legacy txs?
            max_priority_fee_per_gas: Some(
                self.msg.max_priority_fee_per_gas().to_be_bytes().into(),
            ),
            max_fee_per_gas: Some(self.msg.max_fee_per_gas().to_be_bytes().into()),
        }
    }

    pub fn gas_price(&self) -> Option<ethers::types::U256> {
        match self.msg.message {
            Message::Legacy { gas_price, .. } | Message::EIP2930 { gas_price, .. } => {
                Some(gas_price.to_be_bytes().into())
            }
            _ => None,
        }
    }

    fn tx_type(&self) -> Option<ethers::types::U64> {
        match self.msg.message {
            Message::EIP2930 { .. } => Some(1.into()),
            Message::EIP1559 { .. } => Some(2.into()),
            _ => None,
        }
    }

    fn access_list(&self) -> Option<ethers::types::transaction::eip2930::AccessList> {
        match &self.msg.message {
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

/// Converts akula block data into ethers block data
pub struct BlockCast<'a>(pub &'a BlockHeader);
impl<'a> BlockCast<'a> {
    pub fn cast<TX: std::default::Default>(
        &self,
        txs: Vec<TX>,
        block_num: akula::models::BlockNumber,
        block_hash: H256,
        ommer_hashes: Vec<H256>,
    ) -> ethers::types::Block<TX> {
        ethers::types::Block {
            hash: Some(block_hash),
            parent_hash: self.0.parent_hash,
            uncles_hash: self.0.ommers_hash,
            author: self.0.beneficiary,
            state_root: self.0.state_root,
            transactions_root: self.0.transactions_root,
            receipts_root: self.0.receipts_root,
            number: Some(block_num.0.into()),
            gas_used: self.0.gas_used.into(),
            gas_limit: self.0.gas_limit.into(),
            extra_data: self.0.extra_data.clone().into(),
            logs_bloom: Some(self.0.logs_bloom),
            timestamp: self.0.timestamp.into(),
            difficulty: self.0.difficulty.to_be_bytes().into(),
            total_difficulty: None, // TODO
            uncles: ommer_hashes,
            transactions: txs,
            mix_hash: Some(self.0.mix_hash),
            nonce: Some(self.0.nonce.to_fixed_bytes().into()),
            base_fee_per_gas: self.0.base_fee_per_gas.map(|f| f.to_be_bytes().into()),

            // TODO:
            // seal_fields
            //size
            ..Default::default()
        }
    }
}
