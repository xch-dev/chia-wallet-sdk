import test from "ava";
import { readFileSync } from "fs";

import {
  BagAsset,
  BagCoin,
  BagFormat,
  Clvm,
  Coin,
  Constants,
  NftMetadata,
  SecureTheBag,
  SecuredMint,
  Simulator,
  sha256,
} from "../index.js";

interface MintFixture {
  name: string;
  metadata: {
    edition_number: number;
    edition_total: number;
    data_uris: string[];
    data_hash: string | null;
    metadata_uris: string[];
    metadata_hash: string | null;
    license_uris: string[];
    license_hash: string | null;
  };
  royalty_puzzle_hash: string;
  royalty_basis_points: number;
  p2_puzzle_hash: string;
  pre_launcher_parent_id: string;
  expected: {
    pre_launcher_puzzle_hash: string;
    eve_p2_puzzle_hash: string;
    launcher: { coin_id: string };
    eve_nft: { coin_id: string };
    nft: { coin_id: string; puzzle_hash: string };
    launcher_id: string;
  };
}

const fromHex = (value: string) => Buffer.from(value, "hex");
const toHex = (value: Uint8Array) => Buffer.from(value).toString("hex");
const optional = (value: string | null) =>
  value === null ? null : fromHex(value);

const fixtures: {
  mints: MintFixture[];
  bags: { leaf_width: number | null; root_hash: string }[];
} = JSON.parse(
  readFileSync(
    new URL(
      "../../crates/chia-sdk-driver/tests/secure_the_bag/data/secure_the_mint.json",
      import.meta.url,
    ),
    "utf-8",
  ),
);

function securedMint(clvm: Clvm, fixture: MintFixture) {
  const metadata = fixture.metadata;

  return new SecuredMint(
    clvm.nftMetadata(
      new NftMetadata(
        BigInt(metadata.edition_number),
        BigInt(metadata.edition_total),
        metadata.data_uris,
        optional(metadata.data_hash),
        metadata.metadata_uris,
        optional(metadata.metadata_hash),
        metadata.license_uris,
        optional(metadata.license_hash),
      ),
    ),
    Constants.nftMetadataUpdaterDefaultHash(),
    fromHex(fixture.royalty_puzzle_hash),
    fixture.royalty_basis_points,
    fromHex(fixture.p2_puzzle_hash),
  );
}

test("matches the golden vectors through the bindings", (t) => {
  const clvm = new Clvm();

  for (const fixture of fixtures.mints) {
    const mint = securedMint(clvm, fixture);
    const expected = fixture.expected;

    t.is(
      toHex(mint.preLauncherPuzzleHash()),
      expected.pre_launcher_puzzle_hash,
      fixture.name,
    );
    t.is(
      toHex(mint.eveP2PuzzleHash()),
      expected.eve_p2_puzzle_hash,
      fixture.name,
    );

    const output = mint.output(
      new Coin(
        fromHex(fixture.pre_launcher_parent_id),
        mint.preLauncherPuzzleHash(),
        1n,
      ),
    );

    t.is(
      toHex(output.launcher.coinId()),
      expected.launcher.coin_id,
      fixture.name,
    );
    t.is(
      toHex(output.eveNft.coin.coinId()),
      expected.eve_nft.coin_id,
      fixture.name,
    );
    t.is(toHex(output.nft.coin.coinId()), expected.nft.coin_id, fixture.name);
    t.is(toHex(output.nft.info.launcherId), expected.launcher_id, fixture.name);
  }

  const payments = fixtures.mints.map((fixture) =>
    securedMint(clvm, fixture).bagPayment(),
  );

  for (const bag of fixtures.bags) {
    const built = new SecureTheBag(
      BagFormat.legacy(
        bag.leaf_width === null ? null : BigInt(bag.leaf_width),
        false,
      ),
      BagAsset.xch(),
      payments,
    );
    t.is(toHex(built.rootHash()), bag.root_hash);
  }
});

test("mints every nft in a bag", (t) => {
  const sim = new Simulator();
  const clvm = new Clvm();

  const owners = Array.from({ length: 5 }, (_, i) =>
    sha256(Buffer.from(`owner ${i}`)),
  );
  const mints = owners.map(
    (owner, i) =>
      new SecuredMint(
        clvm.nftMetadata(
          new NftMetadata(
            BigInt(i + 1),
            5n,
            [`https://example.com/${i}.png`],
            sha256(Buffer.from([i])),
            [],
            null,
            [],
            null,
          ),
        ),
        Constants.nftMetadataUpdaterDefaultHash(),
        owners[0],
        250,
        owner,
      ),
  );

  const bag = new SecureTheBag(
    BagFormat.legacy(2n, false),
    BagAsset.xch(),
    mints.map((mint) => mint.bagPayment()),
  );

  const alice = sim.bls(bag.totalAmount());
  clvm.spendStandardCoin(
    alice.coin,
    alice.pk,
    clvm.delegatedSpend([
      clvm.createCoin(
        bag.rootHash(),
        bag.totalAmount(),
        clvm.alloc([bag.rootHash()]),
      ),
    ]),
  );

  const root = BagCoin.xch(
    new Coin(alice.coin.coinId(), bag.rootHash(), bag.totalAmount()),
  );
  const preLaunchers = bag.unroll(clvm, root);

  const nfts = mints.map((mint) => {
    const preLauncher = preLaunchers.find(
      (coin) =>
        toHex(coin.p2PuzzleHash()) === toHex(mint.preLauncherPuzzleHash()),
    )!;
    const output = mint.mint(clvm, preLauncher.coin());
    t.deepEqual(output.nft.coin, mint.output(preLauncher.coin()).nft.coin);
    return output.nft;
  });

  sim.spendCoins(clvm.coinSpends(), [alice.sk]);

  nfts.forEach((nft, i) => {
    t.deepEqual(nft.info.p2PuzzleHash, owners[i]);
    t.is(nft.info.royaltyBasisPoints, 250);
    t.deepEqual(sim.hintedCoins(owners[i]), [nft.coin.coinId()]);
  });

  // Minting needs the pre-launcher coin
  t.throws(() => mints[0].mint(clvm, alice.coin));
});
