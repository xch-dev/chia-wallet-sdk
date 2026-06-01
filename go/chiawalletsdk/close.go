package chiawalletsdk

import "io"

// CloseAll closes all items in a slice that implement io.Closer.
// This is useful for cleaning up slices of SDK objects returned
// by methods like CoinSpends(), Children(), etc.
// Each item's Close method must be nil-safe (all SDK types are).
//
//	coinSpends, _ := clvm.CoinSpends()
//	defer CloseAll(coinSpends)
func CloseAll[T io.Closer](items []T) {
	for _, item := range items {
		item.Close()
	}
}
