package main

/*
#include <stdint.h>     // for uintptr_t
*/
import "C"
import "runtime/cgo"
import (
	"context"

	"github.com/holiman/uint256"
	"github.com/ledgerwatch/erigon-lib/kv"
	"github.com/ledgerwatch/erigon-lib/kv/mdbx"
	"github.com/ledgerwatch/erigon/common"
	"github.com/ledgerwatch/erigon/core/rawdb"
	"github.com/ledgerwatch/erigon/core/state"
	"github.com/ledgerwatch/erigon/core/types"
	"github.com/ledgerwatch/erigon/core/types/accounts"
	"github.com/ledgerwatch/erigon/rlp"
	"github.com/ledgerwatch/log/v3"
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
	logger := log.New("Erigon mdbx", path)
	db, err := mdbx.NewMDBX(logger).Path(path).Open()
	if err != nil {
		log.Error("mdbx open", err)
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
}

//export PutAccount
func PutAccount(dbPtr C.uintptr_t, address []byte, rlpAccount []byte, incarnation uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	var acct accounts.Account
	if err := acct.DecodeForHashing(rlpAccount); err != nil {
		log.Error("account DecodeForHashing", err)
		return -1
	}
	acct.Incarnation = incarnation

	tx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	w := state.NewPlainStateWriterNoHistory(tx)
	err = w.UpdateAccountData(common.BytesToAddress(address), new(accounts.Account), &acct)
	if err != nil {
		log.Error("UpdateAccountData", err)
		return -1
	}

	return 1
}

//export PutRawTransactions
func PutRawTransactions(dbPtr C.uintptr_t, txs [][]byte, baseTxId uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	dbtx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	// skip 1 system tx at beginning of write
	err = rawdb.WriteRawTransactions(dbtx, txs, baseTxId+1)
	if err != nil {
		log.Error("WriteRawTransactions", err)
		return -1
	}

	return 1
}

//export PutTransactions
func PutTransactions(dbPtr C.uintptr_t, rlpTxs [][]byte, baseTxId uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	txs, err := types.DecodeTransactions(rlpTxs)
	if err != nil {
		log.Error("DecodeTransactions", err)
		return -1
	}

	dbtx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	// skip 1 system tx at beginning of write
	err = rawdb.WriteTransactions(dbtx, txs, baseTxId+1)
	if err != nil {
		log.Error("WriteTransactions", err)
		return -1
	}

	return 1
}

//export PutBodyForStorage
func PutBodyForStorage(dbPtr C.uintptr_t, hash []byte, num uint64, bodyRlp []byte) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	h := common.BytesToHash(hash)
	body := new(types.BodyForStorage)
	if err := rlp.DecodeBytes(bodyRlp, body); err != nil {
		log.Error("BodyForStorage DecodeBytes", err)
		return -1
	}

	dbtx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteBodyForStorage(dbtx, h, num, body)
	if err != nil {
		log.Error("WriteBodyForStorage", err)
		return -1
	}

	return 1
}

// blockNum is a big.Int
//export PutTxLookupEntries
func PutTxLookupEntries(dbPtr C.uintptr_t, blockNum []byte, txHashes [][]byte) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	dbtx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	for _, hash := range txHashes {
		if err = dbtx.Put(kv.TxLookup, hash, blockNum); err != nil {
			log.Error("failed to store TxLookup entry", "err", err)
		}
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
		log.Error("Overflowed int conversion %x\n", val)
		return -1
	}

	tx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	var acct accounts.Account
	exists, err := rawdb.ReadAccount(tx, who, &acct)
	if err != nil {
		log.Error("ReadAccounts", err)
		return -1
	}

	var incarnation uint64 = 0
	if exists {
		incarnation = acct.Incarnation
	}

	w := state.NewPlainStateWriterNoHistory(tx)
	err = w.WriteAccountStorage(who, incarnation, &k, new(uint256.Int), v)
	if err != nil {
		log.Error("WriteAccountStorage", err)
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
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteHeadHeaderHash(tx, h)
	if err != nil {
		log.Error("WriteHeadHeaderHash", err)
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
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteHeaderNumber(tx, h, num)
	if err != nil {
		log.Error("WriteHeaderNumber", err)
		return -1
	}

	return 1
}

//export PutHeader
func PutHeader(dbPtr C.uintptr_t, headerRlp []byte) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)

	header := new(types.Header)
	if err := rlp.DecodeBytes(headerRlp, header); err != nil {
		log.Error("Header DecodeBytes", err)
		return -1
	}

	tx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	// WriteHeader just log.Crits any errors
	rawdb.WriteHeader(tx, header)

	return 1
}

//export PutCanonicalHash
func PutCanonicalHash(dbPtr C.uintptr_t, hash []byte, num uint64) (exit int) {
	db := cgo.Handle(dbPtr).Value().(kv.RwDB)
	h := common.BytesToHash(hash)

	tx, closer, err := begin(db)
	if err != nil {
		log.Error("tx begin", err)
		return -1
	}
	defer closer(&err)

	err = rawdb.WriteCanonicalHash(tx, h, num)
	if err != nil {
		log.Error("WriteCanonicalHash", err)
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
