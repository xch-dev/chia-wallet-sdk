package chiawalletsdk

import (
	"bytes"
	"context"
	"encoding/hex"
	"errors"
	"io"
	"math"
	"math/big"
	"net"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestAllocIntegerRange(t *testing.T) {
	clvm, err := NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Close()
	for _, tc := range []struct {
		n    int64
		atom string
	}{
		{math.MinInt64, "8000000000000000"},
		{-(1<<53 + 1), "dfffffffffffff"},
		{-256, "ff00"},
		{-129, "ff7f"},
		{-128, "80"},
		{-1, "ff"},
		{0, ""},
		{127, "7f"},
		{128, "0080"},
		{255, "00ff"},
		{1<<53 + 1, "20000000000001"},
		{math.MaxInt64, "7fffffffffffffff"},
	} {
		n := tc.n
		for _, value := range []ClvmValue{ClvmInt(n), ClvmBigInt{V: big.NewInt(n)}} {
			p, err := clvm.Alloc(value)
			if err != nil {
				t.Fatalf("Alloc(%T(%d)): %v", value, n, err)
			}
			encoded, err := p.Int()
			atom, atomErr := p.Atom()
			p.Close()
			if err != nil || bigIntFromSignedBytes(encoded).Cmp(big.NewInt(n)) != 0 {
				t.Fatalf("integer %d: got %x, %v", n, encoded, err)
			}
			if atomErr != nil || hex.EncodeToString(atom) != tc.atom {
				t.Fatalf("integer %d: atom %x, want %s, error %v", n, atom, tc.atom, atomErr)
			}
		}
	}
	if _, err := clvm.Alloc(ClvmBigInt{}); err == nil {
		t.Fatal("expected an error for a nil big.Int")
	}
}

func TestEmptyCollections(t *testing.T) {
	clvm, err := NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Close()
	for _, value := range []ClvmValue{ClvmBytes(nil), ClvmBytes{}, ClvmList(nil), ClvmList{}} {
		p, err := clvm.Alloc(value)
		if err != nil {
			t.Fatalf("Alloc(%T): %v", value, err)
		}
		atom, err := p.Atom()
		p.Close()
		if err != nil || len(atom) != 0 {
			t.Fatalf("empty atom: %x, %v", atom, err)
		}
	}
	for _, spends := range [][]*CoinSpend{nil, {}} {
		response, err := NewGetBlockSpendsResponse(spends, nil, true)
		if err != nil {
			t.Fatal(err)
		}
		got, err := response.BlockSpends()
		response.Close()
		CloseAll(got)
		if err != nil || !reflect.DeepEqual(got, spends) {
			t.Fatalf("optional list: got %#v, want %#v, error %v", got, spends, err)
		}
	}
	memos := [][]byte{nil, {}, bytes.Repeat([]byte{7}, 64)}
	update, err := NewUpdateDatastoreMerkleRoot(make([]byte, 32), memos)
	if err != nil {
		t.Fatal(err)
	}
	defer update.Close()
	got, err := update.Memos()
	if err != nil || len(got) != len(memos) {
		t.Fatalf("memos: %v, %v", got, err)
	}
	for i := range memos {
		if !bytes.Equal(got[i], memos[i]) {
			t.Fatalf("memo %d: got %x, want %x", i, got[i], memos[i])
		}
	}
	// A present but empty hash is invalid; it must not be treated as absent.
	metadata, err := NewHandleNftMetadata(nil, nil, []byte{}, nil, nil, nil, nil)
	if err == nil {
		metadata.Close()
		t.Fatal("empty optional hash was treated as absent")
	}
}

func TestStringsWithNUL(t *testing.T) {
	clvm, err := NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Close()
	text := strings.Repeat("x\x00", 64)
	for _, value := range []string{"", text[1:1], "a\x00b", "雪\x00🌱", text} {
		p, err := clvm.String(value)
		if err != nil {
			t.Fatal(err)
		}
		got, err := p.String()
		atom, atomErr := p.Atom()
		p.Close()
		if err != nil || atomErr != nil || got == nil || *got != value || string(atom) != value {
			t.Fatalf("CLVM string %q: got %v, atom %q, errors %v, %v", value, got, atom, err, atomErr)
		}
		addr, err := NewAddress(make([]byte, 32), value)
		if err != nil {
			t.Fatal(err)
		}
		prefix, err := addr.Prefix()
		addr.Close()
		if err != nil || prefix != value {
			t.Fatalf("string field: got %q, want %q, error %v", prefix, value, err)
		}
		metadata, err := NewHandleNftMetadata(&value, []string{value}, nil, nil, nil, nil, nil)
		if err != nil {
			t.Fatal(err)
		}
		display, err := metadata.DisplayName()
		uris, urisErr := metadata.ImageUris()
		metadata.Close()
		if err != nil || urisErr != nil || display == nil || *display != value || !reflect.DeepEqual(uris, []string{value}) {
			t.Fatalf("optional/list strings: %v, %q, errors %v, %v", display, uris, err, urisErr)
		}
	}
	if p, err := clvm.String(string([]byte{0xff})); err == nil {
		p.Close()
		t.Fatal("invalid UTF-8 must still be rejected")
	}
}

func TestConcurrentArgumentClose(t *testing.T) {
	for range 100 {
		coin, err := NewCoin(make([]byte, 32), make([]byte, 32), 1)
		if err != nil {
			t.Fatal(err)
		}
		start := make(chan struct{})
		var wg sync.WaitGroup
		wg.Go(func() { <-start; coin.Close() })
		wg.Go(func() {
			<-start
			spend, err := NewCoinSpend(coin, []byte{0x80}, []byte{0x80})
			if err == nil {
				spend.Close()
			}
		})
		close(start)
		wg.Wait()
		if spend, err := NewCoinSpend(coin, nil, nil); err == nil {
			spend.Close()
			t.Fatal("closed constructor argument was accepted")
		}
	}
}

func TestConcurrentRepeatedArgumentsClose(t *testing.T) {
	clvm, err := NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Close()
	for range 100 {
		p, err := clvm.Nil()
		if err != nil {
			t.Fatal(err)
		}
		start := make(chan struct{})
		var wg sync.WaitGroup
		wg.Go(func() { <-start; p.Close() })
		wg.Go(func() {
			<-start
			curried, err := p.Curry([]*Program{p, p})
			if err == nil {
				curried.Close()
			}
		})
		close(start)
		wg.Wait()
	}
}

func TestConcurrentSettersAndArguments(t *testing.T) {
	clvm, err := NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Close()
	p, err := clvm.Nil()
	if err != nil {
		t.Fatal(err)
	}
	defer p.Close()
	coin, err := NewCoin(make([]byte, 32), make([]byte, 32), 1)
	if err != nil {
		t.Fatal(err)
	}
	defer coin.Close()
	var wg sync.WaitGroup
	for range 4 {
		wg.Go(func() {
			for range 50 {
				if err := coin.SetAmount(2); err != nil {
					t.Error(err)
				}
				spend, err := NewCoinSpend(coin, nil, nil)
				if err != nil {
					t.Error(err)
					return
				}
				spend.Close()
				// The receiver also appears twice in its argument list.
				curried, err := p.Curry([]*Program{p, p})
				if err != nil {
					t.Error(err)
					return
				}
				curried.Close()
			}
		})
	}
	wg.Wait()
	p.Close()
	if result, err := clvm.CreateCoin(make([]byte, 32), 1, p); err == nil {
		result.Close()
		t.Fatal("closed optional argument was treated as absent")
	}
	if result, err := clvm.List([]*Program{p}); err == nil {
		result.Close()
		t.Fatal("closed list element was accepted")
	}
	if result, err := clvm.List([]*Program{nil}); err == nil {
		result.Close()
		t.Fatal("nil list element was accepted")
	}
}

func TestRPCCancellation(t *testing.T) {
	for _, mode := range []string{"before", "during", "deadline"} {
		t.Run(mode, func(t *testing.T) {
			started := make(chan struct{}, 1)
			disconnected := make(chan struct{}, 1)
			release := make(chan struct{})
			var requests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				io.Copy(io.Discard, r.Body)
				requests.Add(1)
				started <- struct{}{}
				select {
				case <-r.Context().Done():
					disconnected <- struct{}{}
				case <-release:
				}
			}))
			defer server.Close()
			defer close(release)
			client, err := NewRpcClient(server.URL)
			if err != nil {
				t.Fatal(err)
			}
			ctx, cancel := context.WithCancel(context.Background())
			want := context.Canceled
			if mode == "deadline" {
				cancel()
				ctx, cancel = context.WithTimeout(context.Background(), time.Second)
				want = context.DeadlineExceeded
			} else if mode == "before" {
				cancel()
			}
			defer cancel()
			done := make(chan error, 1)
			go func() {
				result, err := client.GetNetworkInfo(ctx)
				result.Close()
				done <- err
			}()
			if mode != "before" {
				select {
				case <-started:
				case <-time.After(5 * time.Second):
					t.Fatal("request never started")
				}
				if mode == "during" {
					cancel()
				}
			}
			select {
			case err := <-done:
				if !errors.Is(err, want) {
					t.Fatalf("got %v, want %v", err, want)
				}
			case <-time.After(5 * time.Second):
				t.Fatal("native operation did not stop after cancellation")
			}
			closed := make(chan struct{})
			go func() { client.Close(); close(closed) }()
			select {
			case <-closed:
			case <-time.After(5 * time.Second):
				t.Fatal("Close blocked after cancellation")
			}
			if mode == "before" {
				if requests.Load() != 0 {
					t.Fatal("already-canceled context sent a request")
				}
				if peer, err := NewPeerConnect(ctx, "testnet11", "127.0.0.1:1", nil, nil); !errors.Is(err, context.Canceled) {
					peer.Close()
					t.Fatalf("async factory: %v", err)
				}
			} else {
				select {
				case <-disconnected:
				case <-time.After(5 * time.Second):
					t.Fatal("HTTP request remained connected after cancellation")
				}
			}
		})
	}
}

func TestPeerConnectCancellation(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	accepted := make(chan net.Conn, 1)
	disconnected := make(chan struct{})
	go func() {
		conn, err := listener.Accept()
		if err != nil {
			return
		}
		defer conn.Close()
		accepted <- conn
		// Consume the TLS handshake without replying, keeping connect pending.
		io.Copy(io.Discard, conn)
		close(disconnected)
	}()
	cert, err := NewCertificateGenerate()
	if err != nil {
		t.Fatal(err)
	}
	defer cert.Close()
	connector, err := NewConnector(cert)
	if err != nil {
		t.Fatal(err)
	}
	options, err := NewPeerOptions()
	if err != nil {
		connector.Close()
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() {
		peer, err := NewPeerConnect(ctx, "testnet11", listener.Addr().String(), connector, options)
		peer.Close()
		done <- err
	}()
	select {
	case conn := <-accepted:
		defer conn.Close()
	case <-time.After(5 * time.Second):
		t.Fatal("peer connection never started")
	}
	closed := make(chan struct{})
	go func() {
		connector.Close()
		options.Close()
		close(closed)
	}()
	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("got %v, want context.Canceled", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("peer connect did not stop after cancellation")
	}
	select {
	case <-closed:
	case <-time.After(5 * time.Second):
		t.Fatal("argument Close blocked after cancellation")
	}
	select {
	case <-disconnected:
	case <-time.After(5 * time.Second):
		t.Fatal("peer socket remained connected after cancellation")
	}
}

// Retain the temporary handle so cleanup can be checked without depending on GC.
type recordedProgram struct{ allocated **Program }

func (v recordedProgram) clvmAlloc(c *Clvm) (*Program, error) {
	p, err := c.Nil()
	*v.allocated = p
	return p, err
}

func TestAllocTemporaryCleanup(t *testing.T) {
	clvm, err := NewClvm()
	if err != nil {
		t.Fatal(err)
	}
	defer clvm.Close()
	for _, fail := range []bool{false, true} {
		for _, pair := range []bool{false, true} {
			var temporary *Program
			var second ClvmValue = ClvmNil{}
			if fail {
				second = nil
			}
			var value ClvmValue = ClvmList{recordedProgram{&temporary}, second}
			if pair {
				value = ClvmPairValue{recordedProgram{&temporary}, second}
			}
			result, err := clvm.Alloc(value)
			if (err != nil) != fail {
				t.Fatalf("Alloc: %v, expected failure: %v", err, fail)
			}
			if result != nil {
				if _, err := result.Serialize(); err != nil {
					t.Fatal(err)
				}
				result.Close()
			}
			if _, err := temporary.Serialize(); err == nil {
				t.Fatal("temporary program was left open")
			}
		}
	}
}
