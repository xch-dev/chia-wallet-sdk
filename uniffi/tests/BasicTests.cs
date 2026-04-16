using System;
using System.Linq;
using Xunit;

namespace ChiaWalletSdk.Tests;

public class BasicTests
{
    [Fact]
    public void ToHexFromHex_Roundtrip()
    {
        var bytes = ChiaWalletSdkMethods.FromHex("ff");
        var hex = ChiaWalletSdkMethods.ToHex(bytes);
        Assert.Equal("ff", hex);
    }

    [Fact]
    public void BytesEqual_Equal()
    {
        var a = new byte[] { 1, 2, 3 };
        var b = new byte[] { 1, 2, 3 };
        Assert.True(ChiaWalletSdkMethods.BytesEqual(a, b));
    }

    [Fact]
    public void BytesEqual_NotEqual()
    {
        var a = new byte[] { 1, 2, 3 };
        var b = new byte[] { 1, 2, 4 };
        Assert.False(ChiaWalletSdkMethods.BytesEqual(a, b));
    }

    [Fact]
    public void CoinId_KnownValue()
    {
        var coin = new Coin(
            ChiaWalletSdkMethods.FromHex("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
            ChiaWalletSdkMethods.FromHex("dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986"),
            "100"
        );
        var coinId = coin.CoinId();
        Assert.Equal(
            "fd3e669c27be9d634fe79f1f7d7d8aaacc3597b855cffea1d708f4642f1d542a",
            ChiaWalletSdkMethods.ToHex(coinId)
        );
    }

    [Fact]
    public void AtomRoundtrip()
    {
        var clvm = new Clvm();
        var expected = new byte[] { 1, 2, 3 };
        var program = clvm.Atom(expected);
        Assert.Equal(expected, program.ToAtom());
    }

    [Fact]
    public void StringRoundtrip()
    {
        var clvm = new Clvm();
        var expected = "hello world";
        var program = clvm.Atom(System.Text.Encoding.UTF8.GetBytes(expected));
        Assert.Equal(expected, program.ToString());
    }

    [Fact]
    public void IntRoundtrip()
    {
        var clvm = new Clvm();
        foreach (var value in new[] { "0", "1", "420", "-1", "-100", "67108863" })
        {
            var program = clvm.Int(value);
            Assert.Equal(value, program.ToInt());
        }
    }

    [Fact]
    public void PairRoundtrip()
    {
        var clvm = new Clvm();
        var first = clvm.Int("1");
        var rest = clvm.Int("100");
        var pair = clvm.Pair(first, rest);
        var result = pair.ToPair();
        Assert.NotNull(result);
        Assert.Equal("1", result.GetFirst().ToInt());
        Assert.Equal("100", result.GetRest().ToInt());
    }

    [Fact]
    public void PublicKeyRoundtrip()
    {
        var original = PublicKey.Infinity();
        var bytes = original.ToBytes();
        var restored = PublicKey.FromBytes(bytes);
        Assert.True(ChiaWalletSdkMethods.BytesEqual(original.ToBytes(), restored.ToBytes()));
    }

    [Fact]
    public void ClvmSerialization()
    {
        var clvm = new Clvm();

        var cases = new (Program program, string hex)[]
        {
            (clvm.Atom(new byte[] { 1, 2, 3 }),         "83010203"),
            (clvm.Int("420"),                             "8201a4"),
            (clvm.Int("100"),                             "64"),
            (clvm.Pair(clvm.Atom(new byte[] { 1, 2, 3 }), clvm.Int("100")), "ff8301020364"),
        };

        foreach (var (program, expectedHex) in cases)
        {
            var serialized = program.Serialize();
            var deserialized = clvm.Deserialize(serialized);
            Assert.Equal(expectedHex, ChiaWalletSdkMethods.ToHex(serialized));
            Assert.True(ChiaWalletSdkMethods.BytesEqual(program.TreeHash(), deserialized.TreeHash()));
        }
    }

    [Fact]
    public void CurryRoundtrip()
    {
        var clvm = new Clvm();
        var items = Enumerable.Range(0, 10).Select(i => clvm.Int(i.ToString())).ToArray();
        var curried = clvm.Nil().Curry(items);
        var uncurried = curried.Uncurry();
        Assert.NotNull(uncurried);
        Assert.True(ChiaWalletSdkMethods.BytesEqual(clvm.Nil().TreeHash(), uncurried.GetProgram().TreeHash()));
        var args = uncurried.GetArgs();
        Assert.NotNull(args);
        var uncurriedArgs = args.Select(p => p.ToInt()!).ToList();
        var expectedArgs = Enumerable.Range(0, 10).Select(i => i.ToString()).ToList();
        Assert.Equal(expectedArgs, uncurriedArgs);
    }

    [Fact]
    public void AllocMultipleTypes()
    {
        var clvm = new Clvm();
        var program = clvm.List(new Program[]
        {
            clvm.Nil(),
            clvm.Alloc(new ClvmType.PublicKey(PublicKey.Infinity())),
            clvm.Atom(System.Text.Encoding.UTF8.GetBytes("Hello, world!")),
            clvm.Int("42"),
            clvm.Int("100"),
            clvm.Bool(true),
            clvm.Atom(new byte[] { 1, 2, 3 }),
            clvm.Atom(new byte[32]),
            clvm.Nil(),
            clvm.Nil(),
            clvm.Alloc(new ClvmType.RunCatTail(new RunCatTail(clvm.Nil(), clvm.Nil()))),
        });

        Assert.Equal(
            "ff80ffb0c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff818fff80ff808080",
            ChiaWalletSdkMethods.ToHex(program.Serialize())
        );
    }

    [Fact]
    public void CreateAndParseCondition()
    {
        var clvm = new Clvm();
        var puzzleHash = new byte[32];
        Array.Fill(puzzleHash, (byte)0xff);

        var memos = clvm.List(new Program[] { clvm.Atom(puzzleHash) });
        var condition = clvm.CreateCoin(puzzleHash, "1", memos);
        var parsed = condition.ParseCreateCoin();

        Assert.NotNull(parsed);
        Assert.True(ChiaWalletSdkMethods.BytesEqual(puzzleHash, parsed.GetPuzzleHash()));
        Assert.Equal("1", parsed.GetAmount());
    }
}
