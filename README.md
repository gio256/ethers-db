# Populate test database

Alternatively, copy an existing erigon db into `./data/` or point the tests at one.

### Start erigon node
```bash
export ERIGON_PATH={}
$ERIGON_PATH/build/bin/erigon --datadir=$PWD/data --chain dev --private.api.addr=localhost:9090 --mine
```

### Start rpc daemon (`erigon/build/bin`)
```bash
$ERIGON_PATH/build/bin/rpcdaemon --datadir=$PWD/data --private.api.addr=localhost:9090 --http.api=eth,erigon,web3,net,debug,trace,txpool,parity
```

### Run tx script
```bash
cargo run --bin txgen
```

# Test block reproduction
```bash
cargo test test_get_block_full -- --nocapture
```
