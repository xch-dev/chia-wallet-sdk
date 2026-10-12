package chiawalletsdk

// The Rust library is linked statically, so that programs using this package don't need to
// ship a shared library alongside them. The prebuilt libraries are included in each release
// of the Go module, and can be built locally with `generate.sh`.

// #cgo CFLAGS: -I${SRCDIR}
// #cgo darwin,amd64 LDFLAGS: ${SRCDIR}/lib/darwin_amd64/libchia_wallet_sdk.a
// #cgo darwin,arm64 LDFLAGS: ${SRCDIR}/lib/darwin_arm64/libchia_wallet_sdk.a
// #cgo darwin LDFLAGS: -framework Security -framework CoreFoundation -framework SystemConfiguration -liconv -lm
// #cgo linux,amd64 LDFLAGS: ${SRCDIR}/lib/linux_amd64/libchia_wallet_sdk.a
// #cgo linux,arm64 LDFLAGS: ${SRCDIR}/lib/linux_arm64/libchia_wallet_sdk.a
// #cgo linux LDFLAGS: -lm -ldl -lpthread -lrt
// #cgo windows,amd64 LDFLAGS: ${SRCDIR}/lib/windows_amd64/libchia_wallet_sdk.a
// #cgo windows LDFLAGS: -lws2_32 -luserenv -lbcrypt -lntdll -lcrypt32 -lsecur32 -lncrypt -ladvapi32 -lkernel32 -lole32 -loleaut32 -lsynchronization
import "C"
