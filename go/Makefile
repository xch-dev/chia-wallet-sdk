.PHONY: generate build test clean

WORKSPACE_ROOT := $(shell cd .. && pwd)

generate:
	cd $(WORKSPACE_ROOT) && cargo run -p go-codegen -- .

build: generate
	cd $(WORKSPACE_ROOT) && cargo build --release -p chia-wallet-sdk-go

test: build
	cd chiawalletsdk && CGO_LDFLAGS="-L$(WORKSPACE_ROOT)/target/release" go test -v ./...

clean:
	cd $(WORKSPACE_ROOT) && cargo clean
