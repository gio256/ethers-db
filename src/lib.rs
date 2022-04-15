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
    use super::{account::Account, ffi::writer::Writer, middleware::DbMiddleware, utils::*};
    use anyhow::{format_err, Result};
    use ethers::{
        core::types::Address,
        providers::{Middleware, MockProvider, Provider},
        utils::keccak256,
    };
    use mdbx::NoWriteMap;
    use once_cell::sync::Lazy;
    use std::{path::PathBuf, sync::Arc};

    const TMP_DIR_ENV_LABEL: &str = "CHAINDATA_TMP_DIR";
    const LINK_TEST_BIN: &str = "LINK_TEST_BIN";

    pub static TMP_DIR: Lazy<PathBuf> = Lazy::new(|| tmp_dir().unwrap());

    fn get_db(path: PathBuf) -> Result<DbMiddleware<impl Middleware, NoWriteMap>> {
        let db = Arc::new(open_db(path)?);
        let provider = Provider::new(MockProvider::new());
        Ok(DbMiddleware::new(provider, db))
    }

    #[tokio::test]
    async fn test_account_accessor() -> Result<()> {
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2".parse()?;
        let acct = Account {
            nonce: 1,
            incarnation: 2,
            balance: ethers::types::U256::MAX,
            codehash: keccak256(vec![0xff]).into(),
        };

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_account(who, acct)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let mut dbtx = db.reader().unwrap();
        let read = dbtx.read_account_data(who).unwrap();
        assert_eq!(acct, read);

        let bal = db.get_balance(who, None).await.unwrap();
        assert_eq!(bal, acct.balance);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_storage_at() -> Result<()> {
        let who: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2".parse()?;
        let key = keccak256(vec![0xff]).into();
        let val = keccak256(vec![0xff, 0xab, 0xcd]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_storage(who, key, val)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let read = db.get_storage_at(who, key, None).await?;
        assert_eq!(read, val);
        Ok(())
    }

    #[tokio::test]
    async fn test_read_head_header_hash() -> Result<()> {
        let hash = keccak256(vec![0xab]).into();

        let mut w = Writer::open(TMP_DIR.clone())?;
        w.put_head_header_hash(hash)?;
        let path = w.close()?;

        let db = get_db(path)?;
        let read = db.reader()?.read_head_header_hash()?;
        assert_eq!(read, hash);
        Ok(())
    }

    fn tmp_dir() -> Result<PathBuf> {
        let path = std::env::var(TMP_DIR_ENV_LABEL).map_err(|e| {
            if std::env::var(LINK_TEST_BIN).is_err() {
                format_err!("Err: {}\nExport {} to run the tests.", e, LINK_TEST_BIN)
            } else {
                format_err!(
                    "Err: {}\nCan't get {}. This is likely a problem with the build script.",
                    e,
                    TMP_DIR_ENV_LABEL
                )
            }
        })?;
        Ok(PathBuf::from(path))
    }

    // #[tokio::test]
    // #[allow(unused)]
    // async fn test_get_block_number() {
    //     let db = get_db();
    //     let num = db
    //         .get_block_number()
    //         .await
    //         .expect("failed to get block number");
    //     dbg!(num);
    // }

    // #[tokio::test]
    // #[allow(unused)]
    // async fn test_get_block_full() {
    //     let db = get_db();
    //     let block = db.get_block(2).await.expect("failed to get block number");
    //     dbg!(block);
    // }

    // #[tokio::test]
    // #[allow(unused)]
    // async fn test_read_code() {
    //     let dst: Address = "0x0d4c6c6605a729a379216c93e919711a081beba2"
    //         .parse()
    //         .unwrap();
    //     let mut dbtx = Reader::new(MDBX.begin().unwrap());
    //     let acct = dbtx.read_account_data(dst).unwrap();
    //     let code = dbtx.read_code(acct.codehash).unwrap();
    //     dbg!(code);
    // }

    // #[tokio::test]
    // #[allow(unused)]
    // async fn test_transactions() {
    //     let db = get_db();
    //     let txs = db.get_block(0).await.unwrap().unwrap().transactions;
    //     dbg!(txs.clone());
    //     let txs = db.get_block(1).await.unwrap().unwrap().transactions;
    //     dbg!(txs.clone());
    //     let txs = db.get_block(2).await.unwrap().unwrap().transactions;
    //     dbg!(txs.clone());
    //     let txs = db.get_block(3).await.unwrap().unwrap().transactions;
    //     dbg!(txs);
    //     let txs = db.get_block(4).await.unwrap().unwrap().transactions;
    //     dbg!(txs);
    // }
}
