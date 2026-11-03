use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{EveProof, Memos, Proof};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{
    Mod,
    conditions::CreateCoin,
    puzzles::{SecureTheMintLauncherArgs, SecureTheMintLauncherSolution},
};
use clvm_traits::{clvm_list, clvm_quote};
use clvm_utils::ToTreeHash;

use crate::{
    BagPayment, DriverError, HashedPtr, Launcher, Nft, NftInfo, NftMint, SingletonInfo, Spend,
    SpendContext,
};

/// An NFT that is minted by spending a one mojo pre-launcher coin, from the secure the mint puzzle.
///
/// The pre-launcher puzzle commits to the NFT's metadata, royalty and owner, so a bag of pre-launcher coins secures
/// every mint in it. Each pre-launcher creates a singleton launcher, which creates an eve NFT that sends itself to
/// [`SecuredMint::p2_puzzle_hash`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecuredMint {
    pub metadata: HashedPtr,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    /// The puzzle hash that the minted NFT is sent to.
    pub p2_puzzle_hash: Bytes32,
    /// The key that can melt the pre-launcher coin instead of minting. Cloud Wallet doesn't set one.
    pub melt_public_key: Option<PublicKey>,
}

/// The coins of a secured mint, from the pre-launcher down to the minted NFT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecuredMintOutput {
    pub pre_launcher: Coin,
    pub launcher: Coin,
    pub eve_nft: Nft,
    pub nft: Nft,
}

impl SecuredMint {
    pub fn new(
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            metadata,
            metadata_updater_puzzle_hash,
            royalty_puzzle_hash,
            royalty_basis_points,
            p2_puzzle_hash,
            melt_public_key: None,
        }
    }

    #[must_use]
    pub fn with_melt_public_key(mut self, melt_public_key: PublicKey) -> Self {
        self.melt_public_key = Some(melt_public_key);
        self
    }

    pub fn pre_launcher_args(&self) -> SecureTheMintLauncherArgs {
        self.pre_launcher_args_with(self.eve_p2_puzzle_hash())
    }

    fn pre_launcher_args_with(&self, eve_p2_puzzle_hash: Bytes32) -> SecureTheMintLauncherArgs {
        SecureTheMintLauncherArgs::new(
            self.metadata.tree_hash().into(),
            self.metadata_updater_puzzle_hash,
            self.royalty_puzzle_hash,
            self.royalty_basis_points,
            eve_p2_puzzle_hash,
            self.melt_public_key,
        )
    }

    pub fn pre_launcher_puzzle_hash(&self) -> Bytes32 {
        self.pre_launcher_args().curry_tree_hash().into()
    }

    /// The payment that creates this mint's pre-launcher coin in a bag.
    pub fn bag_payment(&self) -> BagPayment {
        BagPayment::new(self.pre_launcher_puzzle_hash(), 1)
    }

    /// The p2 puzzle hash of the eve NFT, which sends the NFT to [`SecuredMint::p2_puzzle_hash`] with a hint.
    ///
    /// This is the same p2 puzzle that [`Launcher::mint_nft`] creates when there's no transfer condition.
    pub fn eve_p2_puzzle_hash(&self) -> Bytes32 {
        let create_coin = CreateCoin::new(
            self.p2_puzzle_hash,
            1,
            Memos::Some(clvm_list!(self.p2_puzzle_hash).tree_hash()),
        );
        clvm_quote!(vec![create_coin]).tree_hash().into()
    }

    fn nft_mint(&self) -> NftMint {
        NftMint {
            metadata: self.metadata,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            p2_puzzle_hash: self.p2_puzzle_hash,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_basis_points: self.royalty_basis_points,
            transfer_condition: None,
        }
    }

    /// The coins that minting from the given pre-launcher coin creates, without spending anything.
    pub fn output(&self, pre_launcher: Coin) -> SecuredMintOutput {
        self.output_with(pre_launcher, self.eve_p2_puzzle_hash())
    }

    fn output_with(&self, pre_launcher: Coin, eve_p2_puzzle_hash: Bytes32) -> SecuredMintOutput {
        let launcher = Coin::new(pre_launcher.coin_id(), SINGLETON_LAUNCHER_HASH.into(), 1);

        let eve_info = NftInfo::new(
            launcher.coin_id(),
            self.metadata,
            self.metadata_updater_puzzle_hash,
            None,
            self.royalty_puzzle_hash,
            self.royalty_basis_points,
            eve_p2_puzzle_hash,
        );

        let eve_nft = Nft::new(
            Coin::new(launcher.coin_id(), eve_info.puzzle_hash().into(), 1),
            Proof::Eve(EveProof {
                parent_parent_coin_info: launcher.parent_coin_info,
                parent_amount: launcher.amount,
            }),
            eve_info,
        );

        let nft = eve_nft.child(self.p2_puzzle_hash, None, self.metadata, 1);

        SecuredMintOutput {
            pre_launcher,
            launcher,
            eve_nft,
            nft,
        }
    }

    /// Spends the pre-launcher coin, the launcher and the eve NFT, which sends the NFT to its owner.
    ///
    /// The pre-launcher must have an amount of 1, since the launcher it creates always does.
    pub fn mint(
        &self,
        ctx: &mut SpendContext,
        pre_launcher: Coin,
    ) -> Result<SecuredMintOutput, DriverError> {
        // Any more than this would be lost to the transaction fee
        if pre_launcher.amount != 1 {
            return Err(DriverError::InvalidPreLauncherAmount(pre_launcher.amount));
        }

        let eve_p2_puzzle_hash = self.eve_p2_puzzle_hash();
        self.spend_pre_launcher(ctx, pre_launcher, eve_p2_puzzle_hash, true)?;

        // The pre-launcher creates the launcher and asserts its announcement, so the launcher's parent conditions
        // aren't needed
        let (_, nft) = Launcher::new(pre_launcher.coin_id(), 1).mint_nft(ctx, &self.nft_mint())?;

        let output = self.output_with(pre_launcher, eve_p2_puzzle_hash);
        debug_assert_eq!(nft, output.nft);

        Ok(output)
    }

    /// Spends the pre-launcher coin without minting. This requires a signature from the melt public key on the message
    /// `0x01`, with the `AGG_SIG_ME` additional data.
    pub fn melt(&self, ctx: &mut SpendContext, pre_launcher: Coin) -> Result<(), DriverError> {
        if self.melt_public_key.is_none() {
            return Err(DriverError::NoMeltPublicKey);
        }

        self.spend_pre_launcher(ctx, pre_launcher, self.eve_p2_puzzle_hash(), false)
    }

    fn spend_pre_launcher(
        &self,
        ctx: &mut SpendContext,
        pre_launcher: Coin,
        eve_p2_puzzle_hash: Bytes32,
        mint: bool,
    ) -> Result<(), DriverError> {
        let args = self.pre_launcher_args_with(eve_p2_puzzle_hash);

        if pre_launcher.puzzle_hash != Bytes32::from(args.curry_tree_hash()) {
            return Err(DriverError::NotPreLauncher);
        }

        let puzzle = ctx.curry(args)?;
        let solution = ctx.alloc(&SecureTheMintLauncherSolution::new(
            mint,
            pre_launcher.coin_id(),
        ))?;

        ctx.spend(pre_launcher, Spend::new(puzzle, solution))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_puzzle_types::nft::NftMetadata;
    use chia_puzzles::NFT_METADATA_UPDATER_DEFAULT_HASH;
    use chia_sdk_test::{BlsPair, Simulator};
    use chia_sdk_types::Conditions;

    use crate::{BagAsset, BagCoin, BagFormat, SecureTheBag, StandardLayer};

    use super::*;

    fn secured_mint(ctx: &mut SpendContext, index: u8) -> Result<SecuredMint> {
        let metadata = ctx.alloc_hashed(&NftMetadata {
            edition_number: u64::from(index) + 1,
            edition_total: 10,
            data_uris: vec![format!("https://example.com/{index}.png")],
            data_hash: Some(Bytes32::new([index; 32])),
            ..Default::default()
        })?;

        Ok(SecuredMint::new(
            metadata,
            NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
            Bytes32::new([200; 32]),
            300,
            Bytes32::new([100 + index; 32]),
        ))
    }

    #[test]
    fn test_mint() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let mint = secured_mint(&mut ctx, 0)?;
        let alice = sim.bls(1);

        StandardLayer::new(alice.pk).spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(mint.pre_launcher_puzzle_hash(), 1, Memos::None),
        )?;

        let pre_launcher = Coin::new(alice.coin.coin_id(), mint.pre_launcher_puzzle_hash(), 1);
        let output = mint.mint(&mut ctx, pre_launcher)?;
        assert_eq!(output, mint.output(pre_launcher));

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let nft = output.nft;
        assert_eq!(nft.info.p2_puzzle_hash, mint.p2_puzzle_hash);
        assert_eq!(nft.info.current_owner, None);
        assert_eq!(nft.info.launcher_id, output.launcher.coin_id());
        assert_eq!(nft.info.royalty_basis_points, 300);
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );
        assert_eq!(
            sim.hinted_coins(mint.p2_puzzle_hash),
            vec![nft.coin.coin_id()]
        );

        Ok(())
    }

    #[test]
    fn test_mint_from_bag() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let mints = (0..7)
            .map(|index| secured_mint(&mut ctx, index))
            .collect::<Result<Vec<_>>>()?;

        let bag = SecureTheBag::build(
            BagFormat::Legacy {
                leaf_width: Some(3),
                sorted: false,
            },
            BagAsset::Xch,
            mints.iter().map(SecuredMint::bag_payment),
        )?;

        let alice = sim.bls(bag.total_amount());
        let hint = ctx.hint(bag.root_hash())?;
        StandardLayer::new(alice.pk).spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(bag.root_hash(), bag.total_amount(), hint),
        )?;

        let root = BagCoin::Xch(Coin::new(
            alice.coin.coin_id(),
            bag.root_hash(),
            bag.total_amount(),
        ));
        let pre_launchers = bag.unroll(&mut ctx, root)?;

        for mint in &mints {
            let pre_launcher = pre_launchers
                .iter()
                .find(|coin| coin.p2_puzzle_hash() == mint.pre_launcher_puzzle_hash())
                .unwrap();
            let output = mint.mint(&mut ctx, pre_launcher.coin())?;
            assert_eq!(output.nft.info.p2_puzzle_hash, mint.p2_puzzle_hash);
        }

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        for mint in &mints {
            assert_eq!(sim.hinted_coins(mint.p2_puzzle_hash).len(), 1);
        }

        Ok(())
    }

    #[test]
    fn test_melt() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let melter = BlsPair::new(42);
        let mint = secured_mint(&mut ctx, 0)?;

        // Without a melt key, the pre-launcher can only mint
        let pre_launcher = Coin::new(Bytes32::default(), mint.pre_launcher_puzzle_hash(), 1);
        assert!(matches!(
            mint.melt(&mut ctx, pre_launcher),
            Err(DriverError::NoMeltPublicKey)
        ));

        let mint = mint.with_melt_public_key(melter.pk);
        assert_ne!(mint.pre_launcher_puzzle_hash(), pre_launcher.puzzle_hash);

        let pre_launcher = sim.new_coin(mint.pre_launcher_puzzle_hash(), 1);
        mint.melt(&mut ctx, pre_launcher)?;
        sim.spend_coins(ctx.take(), &[melter.sk])?;

        assert!(
            sim.coin_state(pre_launcher.coin_id())
                .is_some_and(|state| state.spent_height.is_some())
        );
        assert!(sim.children(pre_launcher.coin_id()).is_empty());

        Ok(())
    }

    #[test]
    fn test_rejects_other_coins() -> Result<()> {
        let mut ctx = SpendContext::new();
        let mint = secured_mint(&mut ctx, 0)?;

        let coin = Coin::new(Bytes32::default(), Bytes32::new([1; 32]), 1);
        assert!(matches!(
            mint.mint(&mut ctx, coin),
            Err(DriverError::NotPreLauncher)
        ));

        // The extra mojos would go to the fee
        let coin = Coin::new(Bytes32::default(), mint.pre_launcher_puzzle_hash(), 2);
        assert!(matches!(
            mint.mint(&mut ctx, coin),
            Err(DriverError::InvalidPreLauncherAmount(2))
        ));
        assert!(ctx.take().is_empty());

        Ok(())
    }
}
