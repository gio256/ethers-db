# Run tests
```bash
$ export LINK_TEST_BIN=1
$ cargo +nightly test
```

Running the tests requires a `go` executable to build the bindings in [`dbfaker`](./dbfaker), which are used to write erigon data to test db instances.
The bindings are only used for testing, so`LINK_TEST_BIN` is used to tell the build script when to link them.
