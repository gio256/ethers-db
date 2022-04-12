# Populate test database
### Start erigon node (`erigon/build/bin`)
```bash
./erigon --datadir=$HOME/gio/rs/ethers-db/data --chain dev --private.api.addr=localhost:9090 --mine
```

### Start rpc daemon (`erigon/build/bin`)
```bash
./rpcdaemon --datadir=$HOME/gio/rs/ethers-db/data --private.api.addr=localhost:9090 --http.api=eth,erigon,web3,net,debug,trace,txpool,parity
```

### Run tx script
```bash
cargo run --bin txgen
```

# Test block reproduction
```bash
cargo test test_get_block_full -- --nocapture
```
