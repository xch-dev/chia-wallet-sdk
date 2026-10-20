package chiawalletsdk

import (
	"cmp"
	"slices"
	"sync"
	"unsafe"
)

// Keep the generated lock list independent of argument names such as "sync".
type handleLocks []*sync.RWMutex

// lockHandles locks the receiver and arguments in address order. A handle may
// appear more than once, including as both receiver and argument. Deduplication
// also avoids recursive RLock, which can deadlock when a writer is waiting.
func lockHandles(write *sync.RWMutex, locks ...*sync.RWMutex) func() {
	slices.SortFunc(locks, func(a, b *sync.RWMutex) int {
		return cmp.Compare(uintptr(unsafe.Pointer(a)), uintptr(unsafe.Pointer(b)))
	})
	locks = slices.Compact(locks)
	for _, lock := range locks {
		if lock == write {
			lock.Lock()
		} else {
			lock.RLock()
		}
	}
	return func() {
		for i := len(locks) - 1; i >= 0; i-- {
			if locks[i] == write {
				locks[i].Unlock()
			} else {
				locks[i].RUnlock()
			}
		}
	}
}
