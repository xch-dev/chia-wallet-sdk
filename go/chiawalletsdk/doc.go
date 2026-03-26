// Package chiawalletsdk provides Go bindings for the Chia Wallet SDK.
//
// This package uses CGo to call into a Rust static library that
// implements the core Chia blockchain wallet functionality. Build the
// library first with:
//
//	cd go && make build
//
// # Key types
//
// The main entry point is [Clvm], which owns a CLVM allocator and
// accumulates coin spends. Use it to build programs, parse conditions,
// and construct transactions:
//
//	clvm, _ := chiawalletsdk.ClvmNew()
//	defer clvm.Close()
//
// Cryptographic primitives are provided by [SecretKey], [PublicKey],
// and [Signature] (BLS12-381), as well as secp256k1 ([K1PublicKey],
// [K1Signature]) and secp256r1 ([R1PublicKey], [R1Signature]) variants.
//
// Higher-level spending is handled by [Spends], which tracks selected
// coins and produces [FinishedSpends] ready for signing.
//
// # CLVM allocation
//
// The [Clvm.Alloc] method accepts any [ClvmValue], including primitive
// wrappers ([ClvmInt], [ClvmString], [ClvmBytes], [ClvmBool], [ClvmNil]),
// composite types ([ClvmList], [ClvmPairValue]), and all binding types
// such as [Program], [PublicKey], and condition structs.
//
// # Resource management
//
// Every type that wraps a Rust object implements [io.Closer] and
// registers a runtime finalizer for automatic cleanup. Prefer explicit
// Close calls via defer for deterministic resource management,
// especially in loops or long-lived processes:
//
//	sk, err := chiawalletsdk.NewSecretKeyFromSeed(seed)
//	if err != nil {
//	    return err
//	}
//	defer sk.Close()
//
// Close is idempotent — calling it multiple times is safe. The legacy
// Free method is still available and behaves identically.
//
// # Thread safety
//
// All types are safe for concurrent use from multiple goroutines.
// Each Go wrapper includes a sync.RWMutex that serializes
// Close/Free against concurrent method calls.
//
// Concurrent method calls on the same object are allowed — they
// acquire a shared read lock and the Rust mutex handles serialization.
// Close acquires an exclusive write lock, so it blocks until all
// in-flight method calls complete, then marks the object as closed.
//
// Methods called on a closed object return an error rather than
// panicking. Close itself is idempotent and safe to call from
// multiple goroutines concurrently.
//
// Internally, each FFI call pins the goroutine to its OS thread
// (via runtime.LockOSThread) to ensure thread-local error state is
// retrieved correctly. This is handled automatically and does not
// require any action from callers.
package chiawalletsdk
