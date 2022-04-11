use crate::k256::ecdsa::SigningKey;
use ethers::prelude::*;
use eyre::eyre;

#[tokio::main]
async fn main() {
    let dst: Address = "0xa94f5374Fce5edBC8E2a8697C15331677e6EbF0B"
        .parse()
        .unwrap();

    let endpoint = "http://localhost:8545";
    let provider = Provider::<Http>::try_from(endpoint)
        .map_err(|e| eyre!("Could not establish provider: {}", e))
        .unwrap();
    let chainid = provider.get_chainid().await.unwrap().as_u32() as u16;

    let src: Wallet<SigningKey> =
        "26e86e45f6fc45ec6e2ecd128cec80fa1d1505e5507dcd2ae58c3130a7a97b48"
            .parse()
            .unwrap();
    dbg!(src.address());
    let src = src.with_chain_id(chainid);
    let signer = SignerMiddleware::new(provider, src);

    let tx = TransactionRequest::new().to(dst).value(100_usize);
    let receipt = signer.send_transaction(tx, None).await.unwrap();
    dbg!(receipt);

    println!("foo");
}
