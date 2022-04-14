pub mod middleware;

mod account;
mod db;
mod storage;
mod tables;
mod utils;

#[cfg(test)]
mod ffi;

#[cfg(test)]
mod tests {
    use super::{account::Account, db::*, ffi::ffi, middleware::*, utils::*};
    use akula::kv::mdbx::MdbxEnvironment;
    use ethers::{
        core::types::{Address, },
        providers::{Middleware, MockProvider, Provider},
        utils::keccak256,
    };
    use mdbx::NoWriteMap;
    use once_cell::sync::Lazy;
    use std::{path::PathBuf, sync::Arc};

    pub static MDBX: Lazy<Arc<MdbxEnvironment<mdbx::NoWriteMap>>> = Lazy::new(|| {
        let chaindata_path = env!("ERIGON_CHAINDATA_DIR");
        let chaindata_path = PathBuf::from(chaindata_path);
        ffi::db_init();
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
    async fn test_account_accessor() {
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();
        let acct = Account {
            nonce: 1,
            incarnation: 2,
            balance: ethers::types::U256::MAX,
            codehash: keccak256(vec![0xff]).into(),
        };

        ffi::put_account(who, acct).expect("db seed failed");

        let db = get_db();
        let mut dbtx = db.reader().unwrap();
        let read = dbtx.read_account_data(who).unwrap();
        assert_eq!(acct, read);

        let bal = db.get_balance(who, None).await.unwrap();
        assert_eq!(bal, acct.balance);
    }

    #[tokio::test]
    pub async fn test_get_storage_at() {
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();
        let key = keccak256(vec![0xff]).into();
        let val = keccak256(vec![0xff, 0xab, 0xcd]).into();

        ffi::put_storage(who, key, val).expect("db seed failed");

        let db = get_db();
        let read = db.get_storage_at(who, key, None).await.unwrap();
        assert_eq!(read, val);
    }

    // #[tokio::test]
    #[allow(unused)]
    async fn test_get_block_number() {
        let db = get_db();
        let num = db
            .get_block_number()
            .await
            .expect("failed to get block number");
        dbg!(num);
    }

    // #[tokio::test]
    #[allow(unused)]
    async fn test_get_block_full() {
        let db = get_db();
        let block = db.get_block(2).await.expect("failed to get block number");
        dbg!(block);
    }

    // #[tokio::test]
    #[allow(unused)]
    async fn test_read_code() {
        let dst: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
            .parse()
            .unwrap();
        let mut dbtx = Reader::new(MDBX.begin().unwrap());
        let acct = dbtx.read_account_data(dst).unwrap();
        let code = dbtx.read_code(acct.codehash).unwrap();
        dbg!(code);
    }

    // #[tokio::test]
    #[allow(unused)]
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
    }
}
