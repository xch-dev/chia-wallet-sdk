using System;
using System.Threading.Tasks;
using Xunit;

namespace ChiaWalletSdk.Tests;

public class AsyncTests
{
    [Fact]
    [Trait("Category", "Integration")]
    public async Task GetBlockchainState_AsyncWorks()
    {
        try
        {
            var options = new RpcClientOptions(
                requestTimeoutMs: 30_000,   // whole-request budget: connect + send + receive
                connectTimeoutMs: null);   // just the connection phase

            var rpc = RpcClient.Mainnet().WithOptions(options);
            var response = await rpc.GetBlockchainState();
            Assert.True(response.GetSuccess(), "success should be true");
            Assert.NotNull(response.GetBlockchainState());
        }
        catch (Exception ex) when (IsNetworkError(ex))
        {
            Console.WriteLine($"SKIP: network unavailable: {ex.Message}");
        }
    }

    [Fact]
    [Trait("Category", "Integration")]
    public async Task GetNetworkInfo_AsyncWorks()
    {
        try
        {
            var options = new RpcClientOptions(
                requestTimeoutMs: 30_000,   // whole-request budget: connect + send + receive
                connectTimeoutMs: null);   // just the connection phase

            var rpc = RpcClient.Mainnet().WithOptions(options);
            var response = await rpc.GetNetworkInfo();
            Assert.True(response.GetSuccess(), "success should be true");
            Assert.NotNull(response.GetNetworkName());
        }
        catch (Exception ex) when (IsNetworkError(ex))
        {
            Console.WriteLine($"SKIP: network unavailable: {ex.Message}");
        }
    }

    private static bool IsNetworkError(Exception ex)
    {
        var msg = ex.Message.ToLowerInvariant();
        return msg.Contains("connect") || msg.Contains("timeout") ||
               msg.Contains("network") || msg.Contains("dns") ||
               msg.Contains("request") || msg.Contains("host");
    }
}
