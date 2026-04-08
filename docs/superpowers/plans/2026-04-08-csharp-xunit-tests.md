# C# xUnit Test Project Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create an xUnit test project at `uniffi/tests/` that exercises the C# UniFFI bindings with a suite of tests mirroring the Python and TypeScript binding tests.

**Architecture:** A .NET 10 xUnit project that includes `uniffi/cs/chia_wallet_sdk.cs` directly (same assembly, required for `internal` access), references the pre-built native `libchia_wallet_sdk.dylib`, and tests the core API surface. All types live in namespace `uniffi.chia_wallet_sdk`; free functions are in `ChiaWalletSdkMethods`.

**Tech Stack:** .NET 10, xUnit 2.x, `Microsoft.NET.Test.Sdk`, `xunit.runner.visualstudio`

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `uniffi/tests/ChiaWalletSdkTests.csproj` | Create | Project definition — includes generated .cs, native lib, xUnit packages |
| `uniffi/tests/BasicTests.cs` | Create | All test cases |

---

### Task 1: Create the .csproj

**Files:**
- Create: `uniffi/tests/ChiaWalletSdkTests.csproj`

- [ ] **Step 1: Create the project file**

Create `uniffi/tests/ChiaWalletSdkTests.csproj` with this exact content:

```xml
<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <Nullable>enable</Nullable>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <IsPackable>false</IsPackable>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17.12.0" />
    <PackageReference Include="xunit" Version="2.9.3" />
    <PackageReference Include="xunit.runner.visualstudio" Version="2.8.2">
      <IncludeAssets>runtime; build; native; contentfiles; analyzers; buildtransitive</IncludeAssets>
      <PrivateAssets>all</PrivateAssets>
    </PackageReference>
  </ItemGroup>

  <ItemGroup>
    <!-- Include generated bindings in the same assembly (required for internal access) -->
    <Compile Include="../cs/chia_wallet_sdk.cs" />
  </ItemGroup>

  <ItemGroup>
    <!-- Copy the native library to the test output directory -->
    <None Include="../../target/release/libchia_wallet_sdk.dylib"
          Condition="Exists('../../target/release/libchia_wallet_sdk.dylib')">
      <CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
    </None>
    <None Include="../../target/release/libchia_wallet_sdk.so"
          Condition="Exists('../../target/release/libchia_wallet_sdk.so')">
      <CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
    </None>
    <None Include="../../target/release/chia_wallet_sdk.dll"
          Condition="Exists('../../target/release/chia_wallet_sdk.dll')">
      <CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
    </None>
  </ItemGroup>

</Project>
```

- [ ] **Step 2: Verify the project restores**

```bash
cd uniffi/tests
dotnet restore
```

Expected: packages restore successfully, no errors.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/ChiaWalletSdkTests.csproj
git commit -m "test(cs): add xUnit test project skeleton"
```

---

### Task 2: Write the first failing test — ToHex/FromHex

**Files:**
- Create: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Write the failing test**

Create `uniffi/tests/BasicTests.cs`:

```csharp
using Xunit;
using uniffi.chia_wallet_sdk;

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
}
```

- [ ] **Step 2: Run to verify it fails (project won't compile yet — that's expected)**

```bash
cd uniffi/tests
dotnet build
```

Expected: build succeeds (the test code is valid C# and the generated bindings compile). If the native library is missing you'll get a linker or runtime error — that's a prerequisite, not a test failure. Run `cargo build -p chia-wallet-sdk-cs --release` from the repo root first if needed.

- [ ] **Step 3: Run the test**

```bash
cd uniffi/tests
dotnet test --filter "ToHexFromHex_Roundtrip"
```

Expected: 1 test PASSED.

- [ ] **Step 4: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add ToHex/FromHex roundtrip test"
```

---

### Task 3: BytesEqual

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add equality and inequality tests**

Append inside the `BasicTests` class:

```csharp
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
```

- [ ] **Step 2: Run tests**

```bash
cd uniffi/tests
dotnet test --filter "BytesEqual"
```

Expected: 2 tests PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add BytesEqual tests"
```

---

### Task 4: CoinId

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add coin ID test**

Append inside the `BasicTests` class:

```csharp
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
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "CoinId_KnownValue"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add CoinId known-value test"
```

---

### Task 5: Atom and string roundtrips

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add atom and string roundtrip tests**

Append inside the `BasicTests` class:

```csharp
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
```

- [ ] **Step 2: Run tests**

```bash
cd uniffi/tests
dotnet test --filter "AtomRoundtrip|StringRoundtrip"
```

Expected: 2 tests PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add atom and string roundtrip tests"
```

---

### Task 6: Int roundtrip

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add int roundtrip test**

`Clvm.Int(string)` takes a decimal string. `Program.ToInt()` returns a decimal `string?`.

Append inside the `BasicTests` class:

```csharp
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
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "IntRoundtrip"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add int roundtrip test"
```

---

### Task 7: Pair roundtrip

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add pair roundtrip test**

`Clvm.Pair(first, rest)` returns a `Program`. `Program.ToPair()` returns a `Pair?` record with `First` and `Rest` properties. Verify using `Program.ToInt()` on each element.

Append inside the `BasicTests` class:

```csharp
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
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "PairRoundtrip"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add pair roundtrip test"
```

---

### Task 8: PublicKey roundtrip

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add public key roundtrip test**

Append inside the `BasicTests` class:

```csharp
    [Fact]
    public void PublicKeyRoundtrip()
    {
        var original = PublicKey.Infinity();
        var bytes = original.ToBytes();
        var restored = PublicKey.FromBytes(bytes);
        Assert.True(ChiaWalletSdkMethods.BytesEqual(original.ToBytes(), restored.ToBytes()));
    }
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "PublicKeyRoundtrip"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add PublicKey roundtrip test"
```

---

### Task 9: CLVM serialization

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add serialization test**

`Clvm.Deserialize(byte[])` reconstructs a `Program` from bytes. `Program.Serialize()` produces bytes. `Program.TreeHash()` is stable across serialize/deserialize.

Append inside the `BasicTests` class:

```csharp
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
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "ClvmSerialization"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add CLVM serialization test"
```

---

### Task 10: Curry roundtrip

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add curry roundtrip test**

`Program.Curry(List<Program>)` curries args onto the program. `Program.Uncurry()` returns a `CurriedProgram?` with `Program` and `Args` properties.

Append inside the `BasicTests` class:

```csharp
    [Fact]
    public void CurryRoundtrip()
    {
        var clvm = new Clvm();
        var items = Enumerable.Range(0, 10).Select(i => clvm.Int(i.ToString())).ToList();
        var curried = clvm.Nil().Curry(items);
        var uncurried = curried.Uncurry();
        Assert.NotNull(uncurried);
        Assert.True(ChiaWalletSdkMethods.BytesEqual(clvm.Nil().TreeHash(), uncurried.GetProgram().TreeHash()));
        var args = uncurried.GetArgs();
        Assert.NotNull(args);
        var uncurriedArgs = args.Select(p => p.ToInt()).ToList();
        var expectedArgs = Enumerable.Range(0, 10).Select(i => i.ToString()).ToList();
        Assert.Equal(expectedArgs, uncurriedArgs);
    }
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "CurryRoundtrip"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add curry roundtrip test"
```

---

### Task 11: Alloc with multiple types (mirrors Python test)

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add alloc test**

This mirrors `test_alloc` in `pyo3/tests/test_pyo3.py`. In C#, `Clvm.List(List<Program>)` builds a CLVM proper list (cons chain ending in nil). Primitives use direct methods; complex types use `Clvm.Alloc(ClvmType.*)`.

Append inside the `BasicTests` class:

```csharp
    [Fact]
    public void AllocMultipleTypes()
    {
        var clvm = new Clvm();
        var program = clvm.List(new List<Program>
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
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "AllocMultipleTypes"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add alloc-multiple-types test (mirrors Python test_alloc)"
```

---

### Task 12: Create and parse condition

**Files:**
- Modify: `uniffi/tests/BasicTests.cs`

- [ ] **Step 1: Add condition roundtrip test**

`Clvm.CreateCoin(byte[] puzzleHash, string amount, Program? memos)` creates a condition program. `Program.ParseCreateCoin()` returns a `CreateCoin?` record. `CreateCoin.GetPuzzleHash()` and `CreateCoin.GetAmount()` access fields.

Append inside the `BasicTests` class:

```csharp
    [Fact]
    public void CreateAndParseCondition()
    {
        var clvm = new Clvm();
        var puzzleHash = new byte[32];
        Array.Fill(puzzleHash, (byte)0xff);

        var memos = clvm.List(new List<Program> { clvm.Atom(puzzleHash) });
        var condition = clvm.CreateCoin(puzzleHash, "1", memos);
        var parsed = condition.ParseCreateCoin();

        Assert.NotNull(parsed);
        Assert.True(ChiaWalletSdkMethods.BytesEqual(puzzleHash, parsed.GetPuzzleHash()));
        Assert.Equal("1", parsed.GetAmount());
    }
```

- [ ] **Step 2: Run test**

```bash
cd uniffi/tests
dotnet test --filter "CreateAndParseCondition"
```

Expected: 1 test PASSED.

- [ ] **Step 3: Run all tests together**

```bash
cd uniffi/tests
dotnet test
```

Expected: all 12 tests PASSED.

- [ ] **Step 4: Commit**

```bash
git add uniffi/tests/BasicTests.cs
git commit -m "test(cs): add create-and-parse condition test"
```

---

### Task 13: Update README with test instructions

**Files:**
- Modify: `uniffi/README.md`

- [ ] **Step 1: Add a Testing section to the README**

In `uniffi/README.md`, add this section after "Quick Start" and before "API Surface":

```markdown
## Running the Tests

A cross-platform xUnit test suite lives in `uniffi/tests/`. It exercises the core binding surface — CLVM, keys, coins, addresses, and conditions.

### Prerequisites

Build the native library first (required before `dotnet test` can load it):

```bash
# From the repo root
cargo build -p chia-wallet-sdk-cs --release
```

### Run

```bash
cd uniffi/tests
dotnet test
```

Output should show all tests passing.
```

- [ ] **Step 2: Commit**

```bash
git add uniffi/README.md
git commit -m "docs: add testing section to uniffi README"
```

---

## Prerequisite Reminder

The native library must exist before `dotnet test` will succeed. If tests fail with a `DllNotFoundException` or similar:

```bash
# From the repo root
cargo build -p chia-wallet-sdk-cs --release
```

Then re-run `dotnet test` from `uniffi/tests/`.
