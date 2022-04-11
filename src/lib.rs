#![allow(unused_imports)]

mod account;
mod db;
mod tables;

#[cfg(test)]
mod tests {
    use super::db::*;
    use ethers::{
        core::types::{Address, U64},
        providers::{Middleware, MockProvider, Provider},
    };
    use mdbx::NoWriteMap;

    #[tokio::test]
    async fn test_get_balance() {
        let base_dir = std::env::current_dir().unwrap();
        let data_dir = base_dir.join("data");

        let dst: Address = "0xa94f5374Fce5edBC8E2a8697C15331677e6EbF0B"
            .parse()
            .unwrap();

        let provider = Provider::new(MockProvider::new());
        let db: Db<_, NoWriteMap> = Db::new(provider, data_dir).expect("bad db path");
        let bal = db.get_balance(dst, None).await.unwrap();
        dbg!(bal);
    }

    #[test]
    pub fn test_get_account() {
        let base_dir = std::env::current_dir().unwrap();
        let data_dir = base_dir.join("data");

        let dst: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();

        let provider = Provider::new(MockProvider::new());
        let db: Db<_, NoWriteMap> = Db::new(provider, data_dir).expect("bad db path");
        let acct = db.get_account(dst).unwrap();
        dbg!(acct);
    }

    // #[tokio::test]
    // async fn test_get_block_number() {
    //     let base_dir = std::env::current_dir().unwrap();
    //     let data_dir = base_dir.join("data");

    //     let provider = Provider::new(MockProvider::new());
    //     let db: Db<_, NoWriteMap> = Db::new(provider, data_dir).expect("bad db path");
    //     let num = db.get_block_number().await.expect("failed to get block number");
    //     assert_eq!(num, 2.into());
    // }
}
