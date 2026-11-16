package chiawalletsdk_test

import (
	"bytes"
	"testing"
	"time"

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

func TestPeerSimulatorMempool(t *testing.T) {
	sim := must(sdk.PeerSimulatorStartWithMempool())
	defer sim.Destroy()

	peer := must(sim.ConnectWithMempoolUpdates())
	defer peer.Destroy()

	alice := must(sim.Bls(1000))
	coinID := must(alice.Coin().CoinId())
	must(peer.RegisterForPhUpdates([][]byte{alice.PuzzleHash()}, 0))

	height := must(sim.Height())
	spendBundle := signStandardSpend(t, alice, 900)
	transactionID := must(spendBundle.Hash())

	if ack := must(peer.SendTransaction(spendBundle)); ack.Status() != 1 {
		t.Fatalf("expected the transaction to be accepted, got status %d", ack.Status())
	}

	// Fail instead of hanging if the mempool update never arrives.
	timer := time.AfterFunc(30*time.Second, func() { peer.Close() })
	defer timer.Stop()

	for {
		event := must(peer.Next())
		if event == nil {
			t.Fatal("the connection closed before the mempool update")
		}
		if added := (*event).MempoolItemsAdded(); added != nil {
			ids := (*added).TransactionIds()
			if len(ids) != 1 || !bytes.Equal(ids[0], transactionID) {
				t.Fatal("expected the transaction to be added to the mempool")
			}
			break
		}
	}
	timer.Stop()

	transaction := must(peer.RequestTransaction(transactionID)).Transaction()
	if !bytes.Equal(must(transaction.Hash()), transactionID) {
		t.Fatal("expected the mempool transaction to be returned by its id")
	}

	if records := must(sim.FarmBlock(1)); len(records) != 1 {
		t.Fatalf("expected one block, got %d", len(records))
	}
	if got := must(sim.Height()); got != height+1 {
		t.Fatalf("expected height %d, got %d", height+1, got)
	}

	state := must(sim.CoinState(coinID))
	if state == nil || (*state).SpentHeight() == nil || *(*state).SpentHeight() != height+1 {
		t.Fatal("expected the coin to be spent in the farmed block")
	}
}

func TestPeerSimulatorFarmBlockNeedsMempool(t *testing.T) {
	sim := must(sdk.PeerSimulatorStart())
	defer sim.Destroy()

	if _, err := sim.FarmBlock(1); err == nil {
		t.Fatal("expected farming without a mempool to fail")
	}
}
