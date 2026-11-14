package chiawalletsdk_test

import (
	"strings"
	"testing"

	sdk "github.com/xch-dev/chia-wallet-sdk/go/chiawalletsdk"
)

func TestDaemonClientRejectsInvalidTimeout(t *testing.T) {
	connector := must(sdk.NewConnector(must(sdk.CertificateGenerate())))

	_, err := sdk.DaemonClientConnect("wss://127.0.0.1:1", connector, -1)
	if err == nil || !strings.Contains(err.Error(), "Invalid timeout") {
		t.Fatalf("expected an invalid timeout error, got %v", err)
	}
}
