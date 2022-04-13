package main

import "C"
import (
    // "os"
    "fmt"
    // "path"
    // "log"
    // "context"
    // "math/big"

    // "github.com/ledgerwatch/erigon/core/types"
    // "github.com/ledgerwatch/erigon/core/rawdb"
    // "github.com/ledgerwatch/erigon-lib/kv/mdbx"
	// ledgerLog "github.com/ledgerwatch/log/v3"
)

//export CallMe
func CallMe() {
    fmt.Println("Gogogadget ffi")
}

func main() {}

// output an rlp-encoded list of blocks (maybe even blocks that we've executed?), then check that the
// results are as expected. Can we build blocks in this script, then use importChain to
// execute them?
// func main() {
//     fmt.Println("Hello, world")
//     logger := ledgerLog.New()
//     cwd, err := os.Getwd()
//     if err != nil {
//         log.Fatal(err)
//     }
//     db, err := mdbx.NewMDBX(logger).Path(path.Join(cwd, "chaindata")).Open()
//     if err != nil {
//         log.Fatal(err)
//     }
//     defer db.Close()

//     ctx := context.Background()
//     tx, err := db.BeginRw(ctx)
//     if err != nil {
//         log.Fatal(err)
//     }
//     defer tx.Rollback()

// 	header := &types.Header{Number: big.NewInt(42), Extra: []byte("test header")}
//     rawdb.WriteHeader(tx, header)

//     entry := rawdb.ReadHeader(tx, header.Hash(), header.Number.Uint64())
//     fmt.Printf("%v\n", entry)

//     rawdb.DeleteHeader(tx, header.Hash(), header.Number.Uint64())
// }
