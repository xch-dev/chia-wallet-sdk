.PHONY: generate build test clean install-lib

WORKSPACE_ROOT := $(shell cd .. && pwd)
GOOS := $(shell go env GOOS)
GOARCH := $(shell go env GOARCH)
LIB_DIR := chiawalletsdk/libs/$(GOOS)_$(GOARCH)

generate:
	cd $(WORKSPACE_ROOT) && cargo run -p go-codegen -- .

build: generate
	cd $(WORKSPACE_ROOT) && cargo build --release -p chia-wallet-sdk-go

install-lib: build
	mkdir -p $(LIB_DIR)
	cp $(WORKSPACE_ROOT)/target/release/libchia_wallet_sdk_go.a $(LIB_DIR)/

test: install-lib
	cd chiawalletsdk && go test -v ./...

clean:
	cd $(WORKSPACE_ROOT) && cargo clean
	rm -rf chiawalletsdk/libs/*/libchia_wallet_sdk_go.a
