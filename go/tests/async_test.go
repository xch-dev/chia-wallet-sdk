//go:build integration

package tests

import (
	"strings"
	"testing"

	chia "github.com/xch-dev/chia-wallet-sdk/go/chia_wallet_sdk"
)

func TestAsyncGetBlockchainState(t *testing.T) {
	rpc, err := chia.RpcClientMainnet()
	if err != nil {
		if isNetworkError(err) {
			t.Skipf("network unavailable: %v", err)
		}
		t.Fatal(err)
	}
	defer rpc.Destroy()

	response, err := rpc.GetBlockchainState()
	if err != nil {
		if isNetworkError(err) {
			t.Skipf("network unavailable: %v", err)
		}
		t.Fatal(err)
	}
	defer response.Destroy()

	success, err := response.GetSuccess()
	if err != nil {
		t.Fatal(err)
	}
	if !success {
		t.Error("GetBlockchainState returned success=false")
	}

	state, err := response.GetBlockchainState()
	if err != nil {
		t.Fatal(err)
	}
	if state == nil || *state == nil {
		t.Error("GetBlockchainState returned nil blockchain_state")
	}
}

func TestAsyncGetNetworkInfo(t *testing.T) {
	rpc, err := chia.RpcClientMainnet()
	if err != nil {
		if isNetworkError(err) {
			t.Skipf("network unavailable: %v", err)
		}
		t.Fatal(err)
	}
	defer rpc.Destroy()

	// Bound the whole request so a hung endpoint surfaces as a timeout (treated as
	// a network skip below) instead of blocking the test forever.
	requestTimeoutMs := uint32(30_000)
	options, err := chia.NewRpcClientOptions(&requestTimeoutMs, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer options.Destroy()

	rpcWithOptions, err := rpc.WithOptions(options)
	if err != nil {
		t.Fatal(err)
	}
	defer rpcWithOptions.Destroy()

	response, err := rpcWithOptions.GetNetworkInfo()
	if err != nil {
		if isNetworkError(err) {
			t.Skipf("network unavailable: %v", err)
		}
		t.Fatal(err)
	}
	defer response.Destroy()

	success, err := response.GetSuccess()
	if err != nil {
		t.Fatal(err)
	}
	if !success {
		t.Error("GetNetworkInfo returned success=false")
	}

	name, err := response.GetNetworkName()
	if err != nil {
		t.Fatal(err)
	}
	if name == nil || *name == "" {
		t.Error("GetNetworkInfo returned empty network_name")
	}
}

func isNetworkError(err error) bool {
	msg := strings.ToLower(err.Error())
	return strings.Contains(msg, "connect") || strings.Contains(msg, "timeout") ||
		strings.Contains(msg, "network") || strings.Contains(msg, "dns") ||
		strings.Contains(msg, "request") || strings.Contains(msg, "host")
}
