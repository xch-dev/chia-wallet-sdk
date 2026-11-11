package chiawalletsdk_test

import (
	"bytes"
	"strconv"
	"strings"
	"sync"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

// Ported from the Go bindings in #370. These cover how the generated Go wrappers manage the Rust
// objects they own: destroying them, using them after they're destroyed, and sharing them
// between goroutines.

func TestDestroyIsIdempotent(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	sk.Destroy()
	sk.Destroy()

	if !recoverDestroyed(func() { _, _ = sk.ToBytes() }) {
		t.Fatal("expected the key to stay destroyed")
	}

	var wg sync.WaitGroup
	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 100))

	for range 10 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			coin.Destroy()
		}()
	}

	wg.Wait()

	if !recoverDestroyed(func() { _, _ = sdk.NewCoinSpend(coin, nil, nil) }) {
		t.Fatal("expected the coin to stay destroyed")
	}
}

// recoverDestroyed runs the function, and reports whether it panicked because an object it used
// had already been destroyed. Any other panic is passed on.
func recoverDestroyed(run func()) (destroyed bool) {
	defer func() {
		if r := recover(); r != nil {
			if err, ok := r.(error); ok && strings.Contains(err.Error(), "already been destroyed") {
				destroyed = true
				return
			}
			panic(r)
		}
	}()

	run()
	return false
}

func TestUseAfterDestroyPanics(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(0)))
	sk.Destroy()

	if !recoverDestroyed(func() { _, _ = sk.ToBytes() }) {
		t.Fatal("expected a method call on a destroyed object to panic")
	}

	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1))
	coin.Destroy()

	if !recoverDestroyed(func() { _, _ = sdk.NewCoinSpend(coin, nil, nil) }) {
		t.Fatal("expected passing a destroyed object as an argument to panic")
	}

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	program := must(clvm.Nil())
	program.Destroy()

	if !recoverDestroyed(func() { _, _ = clvm.CreateCoin(make([]byte, 32), 1, &program) }) {
		t.Fatal("expected a destroyed optional argument to panic, not be treated as absent")
	}

	if !recoverDestroyed(func() { _, _ = clvm.List([]*sdk.Program{program}) }) {
		t.Fatal("expected a destroyed list element to panic")
	}
}

// Destroying an object while another goroutine uses it must either let the call finish or make
// it panic with a clear error, and never free the object while a call is using it.
func TestConcurrentDestroyWhileUsing(t *testing.T) {
	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	for range 100 {
		sk := must(sdk.SecretKeyFromSeed(seed(99)))
		coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1))
		program := must(clvm.Nil())

		start := make(chan struct{})
		var wg sync.WaitGroup

		wg.Add(4)
		go func() {
			defer wg.Done()
			<-start
			sk.Destroy()
			coin.Destroy()
			program.Destroy()
		}()
		go func() {
			defer wg.Done()
			<-start
			recoverDestroyed(func() {
				if value, err := sk.ToBytes(); err != nil || len(value) != 32 {
					t.Errorf("unexpected result %x, %v", value, err)
				}
			})
		}()
		go func() {
			defer wg.Done()
			<-start
			recoverDestroyed(func() {
				if spend, err := sdk.NewCoinSpend(coin, []byte{0x80}, []byte{0x80}); err != nil {
					t.Error(err)
				} else {
					spend.Destroy()
				}
			})
		}()
		go func() {
			defer wg.Done()
			<-start
			// The receiver also appears twice in its own argument list
			recoverDestroyed(func() {
				if curried, err := program.Curry([]*sdk.Program{program, program}); err != nil {
					t.Error(err)
				} else {
					curried.Destroy()
				}
			})
		}()

		close(start)
		wg.Wait()
	}
}

func TestConcurrentMethodCalls(t *testing.T) {
	sk := must(sdk.SecretKeyFromSeed(seed(42)))
	defer sk.Destroy()

	clvm := must(sdk.NewClvm())
	defer clvm.Destroy()

	program := must(clvm.Pair(must(clvm.Nil()), must(clvm.Nil())))
	defer program.Destroy()

	coin := must(sdk.NewCoin(make([]byte, 32), make([]byte, 32), 1))
	defer coin.Destroy()

	expectedKey := must(sk.ToBytes())
	expectedHash := must(program.TreeHash())

	var wg sync.WaitGroup
	for i := range 16 {
		wg.Add(1)
		go func() {
			defer wg.Done()

			for j := range 50 {
				clone := sk.Clone()
				value, err := clone.ToBytes()
				clone.Destroy()
				if err != nil || !bytes.Equal(value, expectedKey) {
					t.Errorf("unexpected key %x, %v", value, err)
					return
				}

				hash, err := program.TreeHash()
				if err != nil || !bytes.Equal(hash, expectedHash) {
					t.Errorf("unexpected tree hash %x, %v", hash, err)
					return
				}

				if err := coin.SetAmount(2); err != nil {
					t.Error(err)
					return
				}

				spend, err := sdk.NewCoinSpend(coin, nil, nil)
				if err != nil {
					t.Error(err)
					return
				}
				spend.Destroy()

				// Allocates on the shared Clvm, unlike Clvm.Nil
				number := strconv.Itoa(i*50 + j)
				allocated, err := clvm.Int(number)
				if err != nil {
					t.Error(err)
					return
				}

				got, err := allocated.ToInt()
				allocated.Destroy()
				if err != nil || got == nil || *got != number {
					t.Errorf("expected %s, got %v, %v", number, deref(got), err)
					return
				}
			}
		}()
	}

	wg.Wait()
}
