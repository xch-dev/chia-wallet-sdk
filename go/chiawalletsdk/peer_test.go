package chiawalletsdk_test

import (
	"bytes"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

func TestPeerSimulator(t *testing.T) {
	sim := must(sdk.PeerSimulatorStart())
	defer sim.Destroy()

	peer := must(sim.Connect())
	defer peer.Destroy()

	if addr := must(peer.SocketAddr()); addr != must(sim.SocketAddr()) {
		t.Fatalf("expected the peer to be connected to %s, got %s", must(sim.SocketAddr()), addr)
	}

	alice := must(sim.Bls(1000))
	coinID := must(alice.Coin().CoinId())

	spendBundle := signStandardSpend(t, alice, 900)
	ack := must(peer.SendTransaction(spendBundle))
	if ack.Status() != 1 || ack.Error() != nil {
		t.Fatalf("expected the transaction to be accepted, got status %d and error %v", ack.Status(), deref(ack.Error()))
	}

	children := must(peer.RequestChildren(coinID)).CoinStates()
	if len(children) != 1 || children[0].Coin().Amount() != 900 {
		t.Fatalf("expected one child of 900 mojos, got %d children", len(children))
	}

	transaction := must(peer.RequestTransaction(must(spendBundle.Hash()))).Transaction()
	if !bytes.Equal(must(transaction.Hash()), must(spendBundle.Hash())) {
		t.Fatal("expected the transaction to be returned by its id")
	}

	if peers := must(peer.RequestPeers()).PeerList(); len(peers) != 0 {
		t.Fatalf("expected no peers, got %d", len(peers))
	}

	// Sending the same transaction again is rejected in the ack rather than as an error.
	if ack := must(peer.SendTransaction(spendBundle)); ack.Status() != 3 || ack.Error() == nil {
		t.Fatalf("expected the duplicate transaction to be rejected, got status %d", ack.Status())
	}

	check(t, peer.Close())

	if _, err := peer.RequestChildren(coinID); err == nil {
		t.Fatal("expected a request after closing the peer to fail")
	}
}
