package main

/*
#include <stdint.h>     // for uintptr_t
*/
import "C"
import "runtime/cgo"
import (
    "log"
    "context"

    "github.com/ledgerwatch/erigon/core/types/accounts"
    "github.com/ledgerwatch/erigon/common"
    "github.com/ledgerwatch/erigon/core/rawdb"
    "github.com/ledgerwatch/erigon/core/state"
    "github.com/ledgerwatch/erigon-lib/kv/mdbx"
    "github.com/ledgerwatch/erigon-lib/kv"
    "github.com/holiman/uint256"
	erigonLog "github.com/ledgerwatch/log/v3"
)

func main() {}

// Opens a new mdbx instance at the provided path, returning an ffi-safe
// pointer to a go function which returns this created db instance. This
// pointer should be tracked by the caller and passed into the other methods
// that wish to interact with the same db instance. The pointer (or, the
// cgo.Handle that keeps the pointer alive) consumes resources and must be
// deleted in order for the garbage collector to clean it up (call MdbxClose).
//export MdbxOpen
func MdbxOpen(path string) (exit int, ptr C.uintptr_t) {
    logger := erigonLog.New()
    db, err := mdbx.NewMDBX(logger).Path(path).Open()
    if err != nil {
        log.Print(err)
        return -1, *new(C.uintptr_t)
    }
    f := func() kv.RwDB { return db }
    return 1, C.uintptr_t(cgo.NewHandle(f))
}

// Takes a pointer to a go function returning an mdbx instance, then closes
// that db instance and deletes the pointer handle.
//export MdbxClose
func MdbxClose(ptr C.uintptr_t) {
    handle := cgo.Handle(ptr)
    db := handle.Value().(func() kv.RwDB)()
    db.Close()
    handle.Delete()
    log.Println("Go mdbx closed\n")
}

//export PutAccount
func PutAccount(ptr C.uintptr_t, address []byte, rlpAccount []byte, incarnation uint64) (exit int) {

    db := cgo.Handle(ptr).Value().(func() kv.RwDB)()

    var acct accounts.Account
    if err := acct.DecodeForHashing(rlpAccount); err != nil {
        log.Println(err)
        return -1
    }
    acct.Incarnation = incarnation

    ctx := context.Background()
    tx, err := db.BeginRw(ctx)
    if err != nil {
        log.Println(err)
        return -1
    }
    defer func() {
        if err == nil {
            err = tx.Commit()
        }
        if err != nil {
            tx.Rollback()
        }
    }()

    w := state.NewPlainStateWriterNoHistory(tx)
    err = w.UpdateAccountData(common.BytesToAddress(address), new(accounts.Account), &acct)
    if err != nil {
        log.Println(err)
        return -1
    }

    return 1
}

//export PutStorage
func PutStorage(ptr C.uintptr_t, addressB []byte, keyB []byte, valB []byte) (exit int) {

    db := cgo.Handle(ptr).Value().(func() kv.RwDB)()

    address := common.BytesToAddress(addressB)
    key := common.BytesToHash(keyB)
    val, overflow := uint256.FromBig(common.BytesToHash(valB).Big())
    if overflow {
        log.Printf("Overflowed int conversion %x\n", valB)
        return -1
    }

    ctx := context.Background()
    tx, err := db.BeginRw(ctx)
    if err != nil {
        log.Println(err)
        return -1
    }
    defer func() {
        if err == nil {
            err = tx.Commit()
        }
        if err != nil {
            tx.Rollback()
        }
    }()

    var acct accounts.Account
    exists, err := rawdb.ReadAccount(tx, address, &acct)
    if err != nil {
        log.Println(err)
        return -1
    }

    var incarnation uint64 = 0
    if exists {
        incarnation = acct.Incarnation
    }

    w := state.NewPlainStateWriterNoHistory(tx)
    err = w.WriteAccountStorage(address, incarnation, &key, new(uint256.Int), val)
    if err != nil {
        log.Println(err)
        return -1
    }

    return 1
}
