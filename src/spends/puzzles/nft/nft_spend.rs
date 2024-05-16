use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs, NftStateLayerSolution, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::{
        SingletonArgs, SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    LineageProof, Proof,
};
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;
use sha2::{Digest, Sha256};

use crate::{
    spend_singleton, AssertPuzzleAnnouncement, Chainable, ChainedSpend, CreateCoinWithMemos,
    InnerSpend, NewNftOwner, NftInfo, SpendContext, SpendError, StandardSpend,
};

pub struct NoNftOutput;

pub enum NftOutput {
    SamePuzzleHash,
    NewPuzzleHash { puzzle_hash: Bytes32 },
}

pub struct StandardNftSpend<T> {
    standard_spend: StandardSpend,
    output: T,
    new_owner: Option<NewNftOwner>,
}

impl Default for StandardNftSpend<NoNftOutput> {
    fn default() -> Self {
        Self {
            output: NoNftOutput,
            standard_spend: StandardSpend::new(),
            new_owner: None,
        }
    }
}

impl StandardNftSpend<NoNftOutput> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(self) -> StandardNftSpend<NftOutput> {
        StandardNftSpend {
            standard_spend: self.standard_spend,
            output: NftOutput::SamePuzzleHash,
            new_owner: self.new_owner,
        }
    }

    pub fn transfer(self, puzzle_hash: Bytes32) -> StandardNftSpend<NftOutput> {
        StandardNftSpend {
            standard_spend: self.standard_spend,
            output: NftOutput::NewPuzzleHash { puzzle_hash },
            new_owner: self.new_owner,
        }
    }
}

impl<T> StandardNftSpend<T> {
    pub fn new_owner(mut self, did_id: Bytes32, did_inner_puzzle_hash: Bytes32) -> Self {
        self.new_owner = Some(NewNftOwner {
            new_owner: Some(did_id),
            trade_prices_list: Vec::new(),
            new_did_inner_hash: Some(did_inner_puzzle_hash),
        });
        self
    }
}

impl StandardNftSpend<NftOutput> {
    pub fn finish<M>(
        mut self,
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
        mut nft_info: NftInfo<M>,
    ) -> Result<(ChainedSpend, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let mut chained_spend = ChainedSpend {
            parent_conditions: Vec::new(),
        };

        let p2_puzzle_hash = match self.output {
            NftOutput::SamePuzzleHash => nft_info.p2_puzzle_hash,
            NftOutput::NewPuzzleHash { puzzle_hash } => puzzle_hash,
        };

        if let Some(new_owner) = &self.new_owner {
            self.standard_spend = self.standard_spend.condition(ctx.alloc(new_owner)?);

            let new_nft_owner_args = ctx.alloc(clvm_list!(
                new_owner.new_owner,
                new_owner.trade_prices_list.clone(),
                new_owner.new_did_inner_hash
            ))?;

            let mut announcement_id = Sha256::new();
            announcement_id.update(nft_info.coin.puzzle_hash);
            announcement_id.update([0xad, 0x4c]);
            announcement_id.update(ctx.tree_hash(new_nft_owner_args));

            chained_spend
                .parent_conditions
                .push(ctx.alloc(AssertPuzzleAnnouncement {
                    announcement_id: Bytes32::new(announcement_id.finalize().into()),
                })?);
        }

        let inner_spend = self
            .standard_spend
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash: p2_puzzle_hash,
                amount: nft_info.coin.amount,
                memos: vec![p2_puzzle_hash.to_vec().into()],
            })?)
            .inner_spend(ctx, synthetic_key)?;

        let nft_spend = raw_nft_spend(ctx, &nft_info, inner_spend)?;
        ctx.spend(nft_spend);

        nft_info.current_owner = self
            .new_owner
            .map(|value| value.new_owner)
            .unwrap_or(nft_info.current_owner);

        let metadata_ptr = ctx.alloc(&nft_info.metadata)?;

        let transfer_program = CurriedProgram {
            program: NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
            args: NftRoyaltyTransferPuzzleArgs {
                singleton_struct: SingletonStruct::new(nft_info.launcher_id),
                royalty_puzzle_hash: nft_info.royalty_puzzle_hash,
                trade_price_percentage: nft_info.royalty_percentage,
            },
        };

        let ownership_layer = CurriedProgram {
            program: NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
            args: NftOwnershipLayerArgs {
                mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
                current_owner: nft_info.current_owner,
                transfer_program,
                inner_puzzle: TreeHash::from(p2_puzzle_hash),
            },
        };

        let new_inner_puzzle_hash = CurriedProgram {
            program: NFT_STATE_LAYER_PUZZLE_HASH,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: ctx.tree_hash(metadata_ptr),
                metadata_updater_puzzle_hash: nft_info.metadata_updater_hash,
                inner_puzzle: ownership_layer,
            },
        }
        .tree_hash()
        .into();

        let new_puzzle_hash = CurriedProgram {
            program: SINGLETON_TOP_LAYER_PUZZLE_HASH,
            args: SingletonArgs {
                singleton_struct: SingletonStruct::new(nft_info.launcher_id),
                inner_puzzle: TreeHash::from(new_inner_puzzle_hash),
            },
        }
        .tree_hash()
        .into();

        nft_info.proof = Proof::Lineage(LineageProof {
            parent_parent_coin_id: nft_info.coin.parent_coin_info,
            parent_inner_puzzle_hash: nft_info.nft_inner_puzzle_hash,
            parent_amount: nft_info.coin.amount,
        });

        nft_info.coin = Coin::new(
            nft_info.coin.coin_id(),
            new_puzzle_hash,
            nft_info.coin.amount,
        );

        nft_info.nft_inner_puzzle_hash = new_inner_puzzle_hash;

        Ok((chained_spend, nft_info))
    }
}

impl<T> Chainable for StandardNftSpend<T> {
    fn chain(mut self, chained_spend: ChainedSpend) -> Self {
        self.standard_spend = self.standard_spend.chain(chained_spend);
        self
    }

    fn condition(mut self, condition: NodePtr) -> Self {
        self.standard_spend = self.standard_spend.condition(condition);
        self
    }
}

pub fn raw_nft_spend<M>(
    ctx: &mut SpendContext,
    nft_info: &NftInfo<M>,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let transfer_program_puzzle = ctx.nft_royalty_transfer()?;

    let transfer_program = CurriedProgram {
        program: transfer_program_puzzle,
        args: NftRoyaltyTransferPuzzleArgs {
            singleton_struct: SingletonStruct {
                mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                launcher_id: nft_info.launcher_id,
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            },
            royalty_puzzle_hash: nft_info.royalty_puzzle_hash,
            trade_price_percentage: nft_info.royalty_percentage,
        },
    };

    let ownership_layer_spend =
        spend_nft_ownership_layer(ctx, nft_info.current_owner, transfer_program, inner_spend)?;

    let state_layer_spend = spend_nft_state_layer(
        ctx,
        &nft_info.metadata,
        nft_info.metadata_updater_hash,
        ownership_layer_spend,
    )?;

    spend_singleton(
        ctx,
        nft_info.coin,
        nft_info.launcher_id,
        nft_info.proof,
        state_layer_spend,
    )
}

pub fn spend_nft_state_layer<M>(
    ctx: &mut SpendContext,
    metadata: M,
    metadata_updater_puzzle_hash: Bytes32,
    inner_spend: InnerSpend,
) -> Result<InnerSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let nft_state_layer = ctx.nft_state_layer()?;

    let puzzle = ctx.alloc(CurriedProgram {
        program: nft_state_layer,
        args: NftStateLayerArgs {
            mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
            metadata,
            metadata_updater_puzzle_hash,
            inner_puzzle: inner_spend.puzzle(),
        },
    })?;

    let solution = ctx.alloc(NftStateLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(InnerSpend::new(puzzle, solution))
}

pub fn spend_nft_ownership_layer<P>(
    ctx: &mut SpendContext,
    current_owner: Option<Bytes32>,
    transfer_program: P,
    inner_spend: InnerSpend,
) -> Result<InnerSpend, SpendError>
where
    P: ToClvm<NodePtr>,
{
    let nft_ownership_layer = ctx.nft_ownership_layer()?;

    let puzzle = ctx.alloc(CurriedProgram {
        program: nft_ownership_layer,
        args: NftOwnershipLayerArgs {
            mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
            current_owner,
            transfer_program,
            inner_puzzle: inner_spend.puzzle(),
        },
    })?;

    let solution = ctx.alloc(NftOwnershipLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(InnerSpend::new(puzzle, solution))
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_bls::{sign, Signature};
    use chia_protocol::SpendBundle;
    use chia_puzzles::{
        standard::{StandardArgs, STANDARD_PUZZLE_HASH},
        DeriveSynthetic,
    };
    use clvm_utils::ToTreeHash;
    use clvmr::Allocator;

    use crate::{
        testing::SECRET_KEY, CreateDid, IntermediateLauncher, Launcher, MintNft, RequiredSignature,
        StandardDidSpend, StandardMint, WalletSimulator,
    };

    #[tokio::test]
    async fn test_nft_lineage() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let sk = SECRET_KEY.derive_synthetic();
        let pk = sk.public_key();

        let puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs { synthetic_key: pk },
        }
        .tree_hash()
        .into();

        let parent = sim.generate_coin(puzzle_hash, 2).await.coin;

        let (create_did, did_info) = Launcher::new(parent.coin_id(), 1)
            .create(&mut ctx)?
            .create_standard_did(&mut ctx, pk)?;

        StandardSpend::new()
            .chain(create_did)
            .finish(&mut ctx, parent, pk)?;

        let (mint_nft, mut nft_info) = IntermediateLauncher::new(did_info.coin.coin_id(), 0, 1)
            .create(&mut ctx)?
            .mint_standard_nft(
                &mut ctx,
                StandardMint {
                    metadata: (),
                    royalty_puzzle_hash: puzzle_hash,
                    royalty_percentage: 300,
                    synthetic_key: pk,
                    owner_puzzle_hash: puzzle_hash,
                    did_id: did_info.launcher_id,
                    did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
                },
            )?;

        let mut did_info = StandardDidSpend::new()
            .chain(mint_nft)
            .recreate()
            .finish(&mut ctx, pk, did_info)?;

        for i in 0..5 {
            let mut spend = StandardNftSpend::new().update();

            if i % 2 == 0 {
                spend = spend.new_owner(did_info.launcher_id, did_info.did_inner_puzzle_hash);
            }

            let (nft_spend, new_nft_info) = spend.finish(&mut ctx, pk, nft_info)?;
            nft_info = new_nft_info;

            did_info = StandardDidSpend::new()
                .chain(nft_spend)
                .recreate()
                .finish(&mut ctx, pk, did_info)?;
        }

        let coin_spends = ctx.take_spends();

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?;

        let mut aggregated_signature = Signature::default();

        for required in required_signatures {
            aggregated_signature += &sign(&sk, required.final_message());
        }

        let ack = peer
            .send_transaction(SpendBundle::new(coin_spends, aggregated_signature))
            .await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        let coin_state = peer
            .register_for_coin_updates(vec![did_info.coin.coin_id()], 0)
            .await?
            .remove(0);
        assert_eq!(coin_state.coin, did_info.coin);

        let coin_state = peer
            .register_for_coin_updates(vec![nft_info.coin.coin_id()], 0)
            .await?
            .remove(0);
        assert_eq!(coin_state.coin, nft_info.coin);

        Ok(())
    }
}
