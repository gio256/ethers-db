#![allow(unused_imports)]

pub mod db;

mod account;
mod tables;

#[cfg(test)]
mod tests {
    use super::db::*;
    use ethers::{
        core::types::{Address, H256, U64},
        providers::{Middleware, MockProvider, Provider},
    };
    use mdbx::NoWriteMap;

    const DATA_DIR: &str = "data";

    #[tokio::test]
    async fn test_get_balance() {
        let dst: Address = "0xa94f5374Fce5edBC8E2a8697C15331677e6EbF0B"
            .parse()
            .unwrap();

        let db = get_db();
        let bal = db.get_balance(dst, None).await.unwrap();
        dbg!(bal);
    }

    #[test]
    pub fn test_get_account() {
        let dst: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();

        let db = get_db();
        let acct = db.get_account(dst).unwrap();
        dbg!(acct);
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
        assert_eq!(num, 2.into());
    }

    #[tokio::test]
    async fn test_get_block_header() {
        let db = get_db();
        let res = db.get_header(1).expect("failed to get block number");
    }

    #[tokio::test]
    async fn test_get_block_full() {
        let db = get_db();
        let block = db.get_block(2).await.expect("failed to get block number");
        dbg!(block);
    }

    fn get_db() -> Db<impl Middleware, NoWriteMap> {
        let base_dir = std::env::current_dir().unwrap();
        let data_dir = base_dir.join(DATA_DIR);

        let provider = Provider::new(MockProvider::new());
        Db::new(provider, data_dir).expect("bad db path")
    }
}
