pub mod middleware;

mod account;
mod db;
mod storage;
mod tables;
mod utils;

#[cfg(test)]
mod tests {
    use super::db::*;
    use super::middleware::*;
    use crate::utils::*;
    use akula::kv::mdbx::MdbxEnvironment;
    use ethers::{
        core::types::{Address, H256},
        providers::{Middleware, MockProvider, Provider},
    };
    use mdbx::NoWriteMap;
    use once_cell::sync::Lazy;
    use std::sync::Arc;

    const CHAINDATA_DIR: &str = "data/chaindata";

    pub static MDBX: Lazy<Arc<MdbxEnvironment<mdbx::NoWriteMap>>> = Lazy::new(|| {
        let base_path = std::env::current_dir().expect("could not get pwd");
        let chaindata_path = base_path.join(CHAINDATA_DIR);
        Arc::new(
            open_db(chaindata_path.clone())
                .expect(&format!("could not open erigon db at {:?}", chaindata_path)),
        )
    });

    fn get_db() -> DbMiddleware<impl Middleware, NoWriteMap> {
        let provider = Provider::new(MockProvider::new());
        DbMiddleware::new(provider, MDBX.clone())
    }

    #[tokio::test]
    async fn test_get_balance() {
        let dst: Address = "0xa94f5374Fce5edBC8E2a8697C15331677e6EbF0B"
            .parse()
            .unwrap();

        let db = get_db();
        let bal = db.get_balance(dst, None).await.unwrap();
        dbg!(bal);
    }
    #[tokio::test]
    async fn test_get_balance2() {
        let dst: Address = "0xa94f5374Fce5edBC8E2a8697C15331677e6EbF0B"
            .parse()
            .unwrap();

        let db = get_db();
        let bal = db.get_balance(dst, None).await.unwrap();
        dbg!(bal);
    }

    #[tokio::test]
    pub async fn test_get_storage_at() {
        let dst: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();

        let db = get_db();
        let val = db.get_storage_at(dst, H256::zero(), None).await.unwrap();
        dbg!(val);
    }

    #[tokio::test]
    async fn test_get_block_number() {
        let db = get_db();
        let num = db
            .get_block_number()
            .await
            .expect("failed to get block number");
        dbg!(num);
    }

    #[tokio::test]
    async fn test_get_block_full() {
        let db = get_db();
        let block = db.get_block(2).await.expect("failed to get block number");
        dbg!(block);
    }

    #[tokio::test]
    async fn test_read_code() {
        let dst: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();
        let mut dbtx = DbTx::new(MDBX.begin().unwrap());
        let acct = dbtx.read_account_data(dst).unwrap();
        let code = dbtx.read_code(acct.codehash).unwrap();
        dbg!(code);
    }

    #[tokio::test]
    async fn test_transactions() {
        let db = get_db();
        let txs = db.get_block(0).await.unwrap().unwrap().transactions;
        dbg!(txs.clone());
        let txs = db.get_block(1).await.unwrap().unwrap().transactions;
        dbg!(txs.clone());
        let txs = db.get_block(2).await.unwrap().unwrap().transactions;
        dbg!(txs.clone());
        let txs = db.get_block(3).await.unwrap().unwrap().transactions;
        dbg!(txs);
        let txs = db.get_block(4).await.unwrap().unwrap().transactions;
        dbg!(txs);

        // let mut dbtx = DbTx::new(MDBX.begin().unwrap());
        // let res = dbtx.read_transaction_block_number(txs[1]).unwrap();
        // dbg!(res);
    }
}
