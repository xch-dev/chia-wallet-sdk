import test from "ava";

import {
  Cat,
  CatInfo,
  CatSpend,
  Clvm,
  Coin,
  Constants,
  NotarizedPayment,
  OptionContract,
  OptionInfo,
  OptionMetadata,
  OptionType,
  OptionUnderlying,
  Payment,
  Proof,
  Simulator,
  Spend,
} from "../index.js";

test("mints and spends an option", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const alice = sim.bls(2n);

  const tail = clvm.nil();
  const assetId = tail.treeHash();
  const catInfo = new CatInfo(assetId, null, alice.puzzleHash);

  // Issue a CAT and create a launcher
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(catInfo.puzzleHash(), 1n),
      clvm.createCoin(Constants.singletonLauncherHash(), 0n),
      clvm.createCoin(alice.puzzleHash, 0n),
    ])
  );

  const eve = new Cat(
    new Coin(alice.coin.coinId(), catInfo.puzzleHash(), 1n),
    null,
    catInfo
  );

  clvm.spendCats([
    new CatSpend(
      eve,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([
          clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
          clvm.runCatTail(tail, clvm.nil()),
        ])
      )
    ),
  ]);

  // Lock the CAT as the underlying coin
  const launcher = new Coin(
    alice.coin.coinId(),
    Constants.singletonLauncherHash(),
    0n
  );

  const underlying = new OptionUnderlying(
    launcher.coinId(),
    alice.puzzleHash,
    10n,
    1n,
    OptionType.xch(1n)
  );

  const cat = eve.child(alice.puzzleHash, 1n);

  clvm.spendCats([
    new CatSpend(
      cat,
      clvm.standardSpend(
        alice.pk,
        clvm.delegatedSpend([clvm.createCoin(underlying.puzzleHash(), 1n)])
      )
    ),
  ]);

  const underlyingCat = cat.child(underlying.puzzleHash(), 1n);

  // Spend the launcher
  const optionInfo = new OptionInfo(
    launcher.coinId(),
    underlyingCat.coin.coinId(),
    underlying.delegatedPuzzleHash(),
    alice.puzzleHash
  );

  clvm.spendCoin(
    launcher,
    new Spend(
      clvm.singletonLauncher(),
      clvm.alloc([
        optionInfo.puzzleHash(),
        1n,
        new OptionMetadata(underlying.seconds, underlying.strikeType),
      ])
    )
  );

  const eveOption = new OptionContract(
    new Coin(launcher.coinId(), optionInfo.puzzleHash(), 1n),
    new Proof(launcher.parentCoinInfo, null, launcher.amount),
    optionInfo
  );

  const option = clvm.spendOption(
    eveOption,
    clvm.standardSpend(
      alice.pk,
      clvm.delegatedSpend([
        clvm.createCoin(alice.puzzleHash, 1n, clvm.alloc([alice.puzzleHash])),
      ])
    )
  );

  if (!option) throw new Error("Option not found");

  // Exercise the option using the mojo from melting
  const childCoin = new Coin(alice.coin.coinId(), alice.puzzleHash, 0n);

  const melted = clvm.spendOption(
    option,
    clvm.standardSpend(
      alice.pk,
      clvm.delegatedSpend([
        clvm.meltSingleton(),
        clvm.sendMessage(23, underlying.delegatedPuzzleHash(), [
          clvm.alloc(underlyingCat.coin.coinId()),
        ]),
      ])
    )
  );

  t.is(melted, null);

  clvm.spendStandardCoin(
    childCoin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(Constants.settlementPaymentHash(), 0n),
    ])
  );

  const settlementCoin = new Coin(
    childCoin.coinId(),
    Constants.settlementPaymentHash(),
    0n
  );

  clvm.spendSettlementCoin(settlementCoin, [
    new NotarizedPayment(option.info.launcherId, [
      new Payment(underlying.creatorPuzzleHash, 1n),
    ]),
  ]);

  clvm.spendCats([
    new CatSpend(
      underlyingCat,
      underlying.exerciseSpend(
        clvm,
        option.info.innerPuzzleHash(),
        option.coin.amount
      )
    ),
  ]);

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  t.true(true);
});
