# Run tests
```bash
cargo +nightly test
```

Running the tests requires a `go` executable to build the bindings in [`dbfaker`](./dbfaker), which are used to write erigon data to test db instances.
