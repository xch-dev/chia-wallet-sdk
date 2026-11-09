using ChiaWalletSdk;
using Xunit;

namespace ChiaWalletSdk.Tests;

public class SdkTests
{
    [Fact]
    public void CoinId()
    {
        using var coin = new Coin(
            Convert.FromHexString("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
            Convert.FromHexString("dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986"),
            100
        );

        Assert.Equal(
            Convert.FromHexString("fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a"),
            coin.CoinId()
        );
    }

    [Fact]
    public void SettersMutateInPlace()
    {
        using var coin = new Coin(new byte[32], new byte[32], 100);
        using var clone = coin.Clone();

        coin.SetAmount(ulong.MaxValue);

        Assert.Equal(ulong.MaxValue, coin.Amount());
        Assert.Equal(100UL, clone.Amount());
    }

    [Fact]
    public void WrongLengthThrows()
    {
        var error = Assert.ThrowsAny<ChiaException>(() => new Coin(new byte[31], new byte[32], 100));
        Assert.Contains("expected 32 bytes, found 31", error.Message);
    }

    [Fact]
    public void SingleU64Constructor()
    {
        using var clvm = new Clvm();
        using var condition = new ReserveFee(100);

        Assert.Equal(100UL, condition.Amount());
        Assert.Equal(clvm.ReserveFee(100).TreeHash(), clvm.Alloc(new ClvmType.ReserveFee(condition)).TreeHash());
    }

    [Fact]
    public void BigIntRoundtrip()
    {
        using var clvm = new Clvm();

        foreach (var expected in new[] { "0", "-1", "-421489719874198729487129847", "4384723984791283749823764732649187498237483927482" })
        {
            Assert.Equal(expected, clvm.Int(expected).ToInt());
        }
    }

    [Fact]
    public void ClvmValueAllocation()
    {
        using var clvm = new Clvm();
        using var pk = PublicKey.Infinity();

        var cases = new (ClvmType Value, Program Expected)[]
        {
            (new ClvmType.Nil(), clvm.Nil()),
            (new ClvmType.Bool(true), clvm.Bool(true)),
            (new ClvmType.Int("-42"), clvm.Int("-42")),
            (new ClvmType.Atom([1, 2, 3]), clvm.Atom([1, 2, 3])),
            (new ClvmType.Text("Hello, world!"), clvm.String("Hello, world!")),
            (new ClvmType.PublicKey(pk), clvm.Atom(pk.ToBytes())),
        };

        foreach (var (value, expected) in cases)
        {
            Assert.Equal(expected.TreeHash(), clvm.Alloc(value).TreeHash());
        }
    }

    [Fact]
    public void CurryAddFunction()
    {
        using var clvm = new Clvm();

        var addMod = clvm.Deserialize(Convert.FromHexString("ff10ff02ff0580"));
        var addToTen = addMod.Curry([clvm.Int("10")]);
        using var output = addToTen.Run(clvm.List([clvm.Int("5")]), 10_000_000, true);

        Assert.Equal("15", output.Value().ToInt());
        Assert.Equal(1082UL, output.Cost());
    }

    [Fact]
    public void StaticMethods()
    {
        using var clvm = new Clvm();

        var program = clvm.Deserialize(Sdk.ConstantsP2DelegatedPuzzleOrHiddenPuzzle());

        Assert.Equal(Sdk.ConstantsP2DelegatedPuzzleOrHiddenPuzzleHash(), program.TreeHash());
    }

    [Fact]
    public void SimulatorTransaction()
    {
        using var sim = new Simulator();
        using var clvm = new Clvm();
        using var alice = sim.Bls(1000);

        var spend = clvm.DelegatedSpend([
            clvm.CreateCoin(alice.PuzzleHash(), 900, null),
            clvm.ReserveFee(100),
        ]);

        clvm.SpendStandardCoin(alice.Coin(), alice.Pk(), spend);
        sim.SpendCoins(clvm.CoinSpends(), [alice.Sk()]);

        var coins = sim.UnspentCoins(alice.PuzzleHash(), false);
        Assert.Single(coins);
        Assert.Equal(900UL, coins[0].Amount());
    }

    [Fact]
    public async Task RpcAgainstFullNodeSimulator()
    {
        using var sim = new FullNodeSimulator();
        using var server = await sim.StartServer();

        try
        {
            using var rpc = new RpcClient(server.Url());
            using var info = await rpc.GetNetworkInfo();

            Assert.True(info.Success());
            Assert.Equal("simulator0", info.NetworkName());
        }
        finally
        {
            server.Close();
        }
    }
}
