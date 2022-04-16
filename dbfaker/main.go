package main

/*
#include <stdint.h>     // for uintptr_t
*/
import "C"
import "runtime/cgo"
import (
	"context"
	"log"

	"github.com/holiman/uint256"
	"github.com/ledgerwatch/erigon-lib/kv"
	"github.com/ledgerwatch/erigon-lib/kv/mdbx"
	"github.com/ledgerwatch/erigon/common"
	"github.com/ledgerwatch/erigon/core/rawdb"
	"github.com/ledgerwatch/erigon/core/types"
	"github.com/ledgerwatch/erigon/core/state"
	"github.com/ledgerwatch/erigon/core/types/accounts"
	erigonLog "github.com/ledgerwatch/log/v3"
)

func main() {}

// Opens a new mdbx instance at the provided path, returning an ffi-safe
// pointer the kv.RwDB struct. This pointer should be tracked by the caller
// and passed into the other methods that wish to interact with the same db
// instance. The pointer (or, the cgo.Handle that keeps the pointer alive)
// consumes resources and must be deleted in order for the garbage collector
// to clean it up (call MdbxClose).
//export MdbxOpen
func MdbxOpen(path string) (exit int, ptr C.uintptr_t) {
	logger := erigonLog.New()
	db, err := mdbx.NewMDBX(logger).Path(path).Open()
	if err != nil {
		log.Print(err)
		return -1, *new(C.uintptr_t)
	}
	ptr = C.uintptr_t(cgo.NewHandle(db))
	return 1, ptr
}

// Takes a pointer to a kv.RwDB instance. Closes the db and deletes the pointer handle.
//export MdbxClose
func MdbxClose(dbPtr C.uintptr_t) {
	handle := cgo.Handle(dbPtr)
	db := handle.Value().(kv.RwDB)
	db.Close()
	handle.Delete()
	// log.Println("Go mdbx closed")
}

//export PutAccount
func PutAccount(dbPtr C.uintptr_t, address []byte, rlpAccount []byte, incarnation uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	var acct accounts.Account
	if err := acct.DecodeForHashing(rlpAccount); err != nil {
		log.Println(err)
		return -1
	}
	acct.Incarnation = incarnation

	tx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

	w := state.NewPlainStateWriterNoHistory(tx)
	err = w.UpdateAccountData(common.BytesToAddress(address), new(accounts.Account), &acct)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

//export PutRawTransactions
func PutRawTransactions(dbPtr C.uintptr_t, txs [][]byte, baseTxId uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	dbtx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteRawTransactions(dbtx, txs, baseTxId)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

//export PutTransactions
func PutTransactions(dbPtr C.uintptr_t, rlpTxs [][]byte, baseTxId uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	dbtx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

    txs, err := types.DecodeTransactions(rlpTxs)
	if err != nil {
		log.Println(err)
		return -1
	}

	err = rawdb.WriteTransactions(dbtx, txs, baseTxId)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

//export PutStorage
func PutStorage(dbPtr C.uintptr_t, address []byte, key []byte, val []byte) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	who := common.BytesToAddress(address)
	k := common.BytesToHash(key)
	v, overflow := uint256.FromBig(common.BytesToHash(val).Big())
	if overflow {
		log.Printf("Overflowed int conversion %x\n", val)
		return -1
	}

	tx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

	var acct accounts.Account
	exists, err := rawdb.ReadAccount(tx, who, &acct)
	if err != nil {
		log.Println(err)
		return -1
	}

	var incarnation uint64 = 0
	if exists {
		incarnation = acct.Incarnation
	}

	w := state.NewPlainStateWriterNoHistory(tx)
	err = w.WriteAccountStorage(who, incarnation, &k, new(uint256.Int), v)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

//export PutHeadHeaderHash
func PutHeadHeaderHash(dbPtr C.uintptr_t, hash []byte) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)
	h := common.BytesToHash(hash)

	tx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteHeadHeaderHash(tx, h)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

//export PutHeaderNumber
func PutHeaderNumber(dbPtr C.uintptr_t, hash []byte, num uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)
	h := common.BytesToHash(hash)

	tx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteHeaderNumber(tx, h, num)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

//export PutCanonicalHash
func PutCanonicalHash(dbPtr C.uintptr_t, hash []byte, num uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)
	h := common.BytesToHash(hash)

	tx, closer, err := begin(db)
	if err != nil {
		log.Println(err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteCanonicalHash(tx, h, num)
	if err != nil {
		log.Println(err)
		return -1
	}

	return 1
}

func begin(db kv.RwDB) (tx kv.RwTx, closer func(*error), err error) {
	ctx := context.Background()
	tx, err = db.BeginRw(ctx)
	if err != nil {
		return nil, nil, err
	}

	closer = func(e *error) {
		if *e == nil {
			*e = tx.Commit()
		}
		if *e != nil {
			tx.Rollback()
		}
	}
	return tx, closer, nil
}
