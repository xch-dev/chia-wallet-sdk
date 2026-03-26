// Package chiawalletsdk provides Go bindings for the Chia Wallet SDK.
//
// This package uses CGo to call into a Rust shared library that
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
//	defer clvm.Free()
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
// # Memory management
//
// Every type that wraps a Rust object carries a [Free] method and
// registers a runtime finalizer for automatic cleanup. Explicit [Free]
// calls are recommended for deterministic resource management,
// especially in loops or long-lived processes.
package chiawalletsdk
