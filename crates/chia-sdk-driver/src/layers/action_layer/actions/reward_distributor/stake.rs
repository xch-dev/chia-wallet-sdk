use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonStruct,
};
use chia_puzzle_types::{
    nft::NftRoyaltyTransferPuzzleArgs,
    offer::{NotarizedPayment, Payment},
    LineageProof,
};
use chia_puzzles::{NFT_OWNERSHIP_LAYER_HASH, NFT_STATE_LAYER_HASH, SETTLEMENT_PAYMENT_HASH};
use chia_wallet_sdk::{
    driver::{Asset, DriverError, HashedPtr, Nft, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{clvm_tuple, FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, P2DelegatedBySingletonLayerArgs, RewardDistributor, RewardDistributorConstants,
    RewardDistributorEntrySlotValue, RewardDistributorSlotNonce, RewardDistributorState, Slot,
    SpendContextExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardDistributorStakeAction {
    pub launcher_id: Bytes32,
    pub did_launcher_id: Bytes32,
    pub max_second_offset: u64,
}

impl ToTreeHash for RewardDistributorStakeAction {
    fn tree_hash(&self) -> TreeHash {
        RewardDistributorStakeActionArgs::curry_tree_hash(
            self.launcher_id,
            self.did_launcher_id,
            self.max_second_offset,
        )
    }
}

impl Action<RewardDistributor> for RewardDistributorStakeAction {
    fn from_constants(constants: &RewardDistributorConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            did_launcher_id: constants.manager_or_collection_did_launcher_id,
            max_second_offset: constants.max_seconds_offset,
        }
    }
}

impl RewardDistributorStakeAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.reward_distributor_stake_action_puzzle()?,
            args: RewardDistributorStakeActionArgs::new(
                self.launcher_id,
                self.did_launcher_id,
                self.max_second_offset,
            ),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn created_slot_value(
        ctx: &SpendContext,
        state: &RewardDistributorState,
        solution: NodePtr,
    ) -> Result<RewardDistributorEntrySlotValue, DriverError> {
        let solution = ctx.extract::<RewardDistributorStakeActionSolution>(solution)?;

        Ok(RewardDistributorEntrySlotValue {
            payout_puzzle_hash: solution.entry_custody_puzzle_hash,
            initial_cumulative_payout: state.round_reward_info.cumulative_payout,
            shares: 1,
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        distributor: &mut RewardDistributor,
        current_nft: Nft<HashedPtr>,
        nft_launcher_proof: NftLauncherProof,
        entry_custody_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, NotarizedPayment, Nft<HashedPtr>), DriverError> {
        let ephemeral_counter =
            ctx.extract::<HashedPtr>(distributor.pending_spend.latest_state.0)?;
        let my_id = distributor.coin.coin_id();

        // calculate notarized payment
        let my_p2_treehash: TreeHash =
            RewardDistributorStakeActionArgs::my_p2_puzzle_hash(self.launcher_id).into();
        let payment_puzzle_hash: Bytes32 = CurriedProgram {
            program: NONCE_WRAPPER_PUZZLE_HASH,
            args: NonceWrapperArgs::<Bytes32, TreeHash> {
                nonce: entry_custody_puzzle_hash,
                inner_puzzle: my_p2_treehash,
            },
        }
        .tree_hash()
        .into();
        let notarized_payment = NotarizedPayment {
            nonce: clvm_tuple!(ephemeral_counter.tree_hash(), my_id)
                .tree_hash()
                .into(),
            payments: vec![Payment::new(
                payment_puzzle_hash,
                1,
                ctx.hint(payment_puzzle_hash)?,
            )],
        };

        // spend self
        let nft = current_nft.child(
            SETTLEMENT_PAYMENT_HASH.into(),
            None,
            current_nft.info.metadata,
            current_nft.amount(),
        );
        let action_solution = ctx.alloc(&RewardDistributorStakeActionSolution {
            my_id,
            nft_metadata_hash: nft.info.metadata.tree_hash().into(),
            nft_metadata_updater_hash_hash: nft
                .info
                .metadata_updater_puzzle_hash
                .tree_hash()
                .into(),
            nft_transfer_porgram_hash: NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
                nft.info.launcher_id,
                nft.info.royalty_puzzle_hash,
                nft.info.royalty_basis_points,
            )
            .into(),
            nft_launcher_proof,
            entry_custody_puzzle_hash,
        })?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
        let msg: Bytes32 = ctx.tree_hash(notarized_payment_ptr).into();
        distributor.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        Ok((
            Conditions::new()
                .assert_puzzle_announcement(announcement_id(nft.coin.puzzle_hash, msg)),
            notarized_payment,
            nft.child(payment_puzzle_hash, None, nft.info.metadata, nft.amount()),
        ))
    }
}

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE: [u8; 1170] = hex!("ff02ffff01ff04ffff04ffff10ff8209ffffff010180ffff04ff8215ffffff04ffff10ff822dffffff010180ffff04ff825dffffff04ff82bdffff808080808080ffff02ff3cffff04ff02ffff04ffff0bffff02ff3affff04ff02ffff04ff09ffff04ffff02ff3effff04ff02ffff04ffff04ff09ffff04ffff02ff36ffff04ff02ffff04ffff30ff83047bffffff02ff3affff04ff02ffff04ff09ffff04ffff02ff3effff04ff02ffff04ff05ff80808080ffff04ff830a7bffff808080808080ff83167bff80ffff04ff83037bffff8080808080ff1d8080ff80808080ffff04ffff02ff3affff04ff02ffff04ff0bffff04ffff0bffff0101ff0b80ffff04ff822bffffff04ff825bffffff04ffff02ff3affff04ff02ffff04ff17ffff04ffff0bffff0101ff1780ffff04ff8192ffff04ff82bbffffff04ff2fff8080808080808080ff8080808080808080ff808080808080ffff02ff3effff04ff02ffff04ffff04ffff02ff3effff04ff02ffff04ffff04ff8209ffff8213ff80ff80808080ffff04ffff02ff2effff04ff02ffff04ffff02ff3affff04ff02ffff04ff5fffff04ffff0bffff0101ff8301fbff80ffff04ff81bfff808080808080ff80808080ff808080ff8080808080ffff04ffff04ffff04ff28ffff04ff8213ffff808080ffff04ffff02ff2affff04ff02ffff04ff82017fffff04ffff02ff3effff04ff02ffff04ffff04ff8301fbffffff04ff829dffffff01018080ff80808080ffff04ff8301fbffff808080808080ffff04ffff04ff10ffff04ffff10ff83013dffff8202ff80ff808080ff80808080ff808080808080ffff04ffff01ffffff55ff463fffff333eff02ff04ffff04ff38ffff04ff05ff808080ffff04ffff04ff34ffff04ff05ff808080ff0b8080ffffffff02ffff03ff05ffff01ff0bff81f2ffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff22ffff04ff02ffff04ff0dff80808080ff808080808080ffff0181d280ff0180ffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ff24ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff17ff8080ff8080808080ff0bff81b2ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff22ffff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff2cffff0bff2cff81d2ff0580ffff0bff2cff0bff81928080ff02ffff03ff0bffff01ff30ffff02ff36ffff04ff02ffff04ff05ffff04ff1bff8080808080ff23ff3380ffff010580ff0180ffff04ff05ffff04ffff0101ffff04ffff04ff05ff8080ff80808080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b092c8a9a97f69a906230663bffaf52a6d435ee57fd93a5e84862a1f935ea101
    "
));

// run '(mod (NONCE INNER_PUZZLE . inner_solution) (a INNER_PUZZLE inner_solution))' -d
pub const NONCE_WRAPPER_PUZZLE: [u8; 7] = hex!("ff02ff05ff0780");
pub const NONCE_WRAPPER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "847d971ef523417d555ea9854b1612837155d34d453298defcd310774305f657"
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct NonceWrapperArgs<N, I> {
    pub nonce: N,
    pub inner_puzzle: I,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorStakeActionArgs {
    pub did_singleton_struct: SingletonStruct,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub offer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
}

impl RewardDistributorStakeActionArgs {
    pub fn new(launcher_id: Bytes32, did_launcher_id: Bytes32, max_second_offset: u64) -> Self {
        Self {
            did_singleton_struct: SingletonStruct::new(did_launcher_id),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            offer_mod_hash: SETTLEMENT_PAYMENT_HASH.into(),
            nonce_mod_hash: NONCE_WRAPPER_PUZZLE_HASH.into(),
            my_p2_puzzle_hash: Self::my_p2_puzzle_hash(launcher_id),
            entry_slot_1st_curry_hash: Slot::<()>::first_curry_hash(
                launcher_id,
                RewardDistributorSlotNonce::ENTRY.to_u64(),
            )
            .into(),
            max_second_offset,
        }
    }

    pub fn my_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
        P2DelegatedBySingletonLayerArgs::curry_tree_hash(
            SingletonStruct::new(launcher_id).tree_hash().into(),
            1,
        )
        .into()
    }
}

impl RewardDistributorStakeActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        did_launcher_id: Bytes32,
        max_second_offset: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH,
            args: RewardDistributorStakeActionArgs::new(
                launcher_id,
                did_launcher_id,
                max_second_offset,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct IntermediaryCoinProof {
    pub full_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub amount: u64,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct NftLauncherProof {
    pub did_proof: LineageProof,
    #[clvm(rest)]
    pub intermediary_coin_proofs: Vec<IntermediaryCoinProof>,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorStakeActionSolution {
    pub my_id: Bytes32,
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_transfer_porgram_hash: Bytes32,
    pub nft_launcher_proof: NftLauncherProof,
    #[clvm(rest)]
    pub entry_custody_puzzle_hash: Bytes32,
}
