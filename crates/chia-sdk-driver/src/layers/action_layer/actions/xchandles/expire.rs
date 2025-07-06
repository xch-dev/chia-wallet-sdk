use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonStruct,
};
use chia_wallet_sdk::{
    driver::{DriverError, Spend, SpendContext},
    types::{announcement_id, Conditions},
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{
    Action, DefaultCatMakerArgs, PrecommitCoin, PrecommitLayer, Slot, SlotNeigborsInfo,
    SpendContextExt, XchandlesConstants, XchandlesDataValue, XchandlesPrecommitValue,
    XchandlesRegistry, XchandlesSlotValue,
};

use super::{XchandlesFactorPricingPuzzleArgs, XchandlesPricingSolution};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XchandlesExpireAction {
    pub launcher_id: Bytes32,
    pub relative_block_height: u32,
    pub payout_puzzle_hash: Bytes32,
}

impl ToTreeHash for XchandlesExpireAction {
    fn tree_hash(&self) -> TreeHash {
        XchandlesExpireActionArgs::curry_tree_hash(
            self.launcher_id,
            self.relative_block_height,
            self.payout_puzzle_hash,
        )
    }
}

impl Action<XchandlesRegistry> for XchandlesExpireAction {
    fn from_constants(constants: &XchandlesConstants) -> Self {
        Self {
            launcher_id: constants.launcher_id,
            relative_block_height: constants.relative_block_height,
            payout_puzzle_hash: constants.precommit_payout_puzzle_hash,
        }
    }
}

impl XchandlesExpireAction {
    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.xchandles_expire_puzzle()?,
            args: XchandlesExpireActionArgs::new(
                self.launcher_id,
                self.relative_block_height,
                self.payout_puzzle_hash,
            ),
        }
        .to_clvm(ctx)?)
    }

    pub fn spent_slot_value(
        ctx: &SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        // truths for epired solution are: Buy_Time, Current_Expiration, Handle
        let solution = XchandlesExpireActionSolution::<
            NodePtr,
            NodePtr,
            NodePtr,
            (NodePtr, (u64, (String, NodePtr))),
            NodePtr,
        >::from_clvm(ctx, solution)?;

        let handle = solution.expired_handle_pricing_puzzle_solution.1 .1 .0;
        let current_expiration = solution.expired_handle_pricing_puzzle_solution.1 .0;

        Ok(XchandlesSlotValue::new(
            handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            current_expiration,
            solution.old_rest.owner_launcher_id,
            solution.old_rest.resolved_data,
        ))
    }

    pub fn created_slot_value(
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<XchandlesSlotValue, DriverError> {
        let solution = ctx.extract::<XchandlesExpireActionSolution<
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
            NodePtr,
        >>(solution)?;

        let pricing_output = ctx.run(
            solution.expired_handle_pricing_puzzle_reveal,
            solution.expired_handle_pricing_puzzle_solution,
        )?;
        let registration_time_delta = <(NodePtr, u64)>::from_clvm(ctx, pricing_output)?.1;

        // truths are: Buy_Time, Current_Expiration, Handle
        let (buy_time, (_, (handle, _))) = ctx.extract::<(u64, (NodePtr, (String, NodePtr)))>(
            solution.expired_handle_pricing_puzzle_solution,
        )?;

        Ok(XchandlesSlotValue::new(
            handle.tree_hash().into(),
            solution.neighbors.left_value,
            solution.neighbors.right_value,
            buy_time + registration_time_delta,
            solution.new_rest.owner_launcher_id,
            solution.new_rest.resolved_data,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spend(
        self,
        ctx: &mut SpendContext,
        registry: &mut XchandlesRegistry,
        slot: Slot<XchandlesSlotValue>,
        num_periods: u64,
        base_handle_price: u64,
        registration_period: u64,
        precommit_coin: PrecommitCoin<XchandlesPrecommitValue>,
        start_time: u64,
    ) -> Result<Conditions, DriverError> {
        let my_inner_puzzle_hash: Bytes32 = registry.info.inner_puzzle_hash().into();

        // announcement is simply premcommitment coin ph
        let expire_ann: Bytes32 = precommit_coin.coin.puzzle_hash;

        // spend precommit coin
        precommit_coin.spend(
            ctx,
            1, // mode 1 = register/expire (use value)
            my_inner_puzzle_hash,
        )?;

        // spend self
        let slot = registry.actual_slot(slot);
        let action_solution = XchandlesExpireActionSolution {
            cat_maker_puzzle_reveal: DefaultCatMakerArgs::get_puzzle(
                ctx,
                precommit_coin.asset_id.tree_hash().into(),
            )?,
            cat_maker_puzzle_solution: (),
            expired_handle_pricing_puzzle_reveal:
                XchandlesExponentialPremiumRenewPuzzleArgs::from_scale_factor(
                    ctx,
                    base_handle_price,
                    registration_period,
                    1000,
                )?
                .get_puzzle(ctx)?,
            expired_handle_pricing_puzzle_solution: XchandlesPricingSolution {
                buy_time: start_time,
                current_expiration: slot.info.value.expiration,
                handle: precommit_coin.value.handle.clone(),
                num_periods,
            },
            refund_puzzle_hash_hash: precommit_coin.refund_puzzle_hash.tree_hash().into(),
            secret: precommit_coin.value.secret,
            neighbors: slot.info.value.neighbors,
            old_rest: slot.info.value.rest_data(),
            new_rest: XchandlesDataValue {
                owner_launcher_id: precommit_coin.value.owner_launcher_id,
                resolved_data: precommit_coin.value.resolved_data,
            },
        }
        .to_clvm(ctx)?;
        let action_puzzle = self.construct_puzzle(ctx)?;

        registry.insert_action_spend(ctx, Spend::new(action_puzzle, action_solution))?;

        // spend slot
        slot.spend(ctx, my_inner_puzzle_hash)?;

        let mut expire_ann: Vec<u8> = expire_ann.to_vec();
        expire_ann.insert(0, b'x');
        Ok(Conditions::new()
            .assert_puzzle_announcement(announcement_id(registry.coin.puzzle_hash, expire_ann)))
    }
}

pub const XCHANDLES_EXPIRE_PUZZLE: [u8; 1073] =
    hex!("ff02ffff01ff02ffff03ffff22ffff09ffff02ff16ffff04ff02ffff04ff4fff80808080ff5780ffff09ffff02ff16ffff04ff02ffff04ff82016fff80808080ff81f780ffff09ffff0dff825fef80ffff012080ffff15ffff0141ffff0dff827fef808080ffff01ff04ff17ffff02ff2effff04ff02ffff04ffff02ff4fffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ff8205ef80ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ffff02ff16ffff04ff02ffff04ffff04ffff04ffff04ff57ff81af80ffff04ff81f7ff8202ef8080ffff04ffff04ff8216efff820bef80ffff04ff825fefff827fef808080ff808080808080ffff0bff3cff62ff42808080ff42808080ff42808080ff81af8080ffff04ffff05ffff02ff82016fff8202ef8080ffff04ffff04ffff04ff10ffff04ff8204efff808080ffff04ffff04ff10ffff04ff820aefff808080ffff04ffff02ff3effff04ff02ffff04ff0bffff04ffff02ff16ffff04ff02ffff04ffff04ffff04ffff0bffff0101ff8216ef80ff8217ef80ffff04ff820aefff822fef8080ff80808080ff8080808080ffff04ffff02ff1affff04ff02ffff04ff0bffff04ffff02ff16ffff04ff02ffff04ffff04ffff04ffff0bffff0101ff8216ef80ff8217ef80ffff04ffff10ffff06ffff02ff82016fff8202ef8080ff8204ef80ff823fef8080ff80808080ff8080808080ff8080808080ff80808080808080ffff01ff088080ff0180ffff04ffff01ffffff5133ff3eff4202ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff04ff18ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ffff04ffff04ff2cffff04ffff0113ffff04ffff0101ffff04ff05ffff04ff0bff808080808080ffff04ffff04ff14ffff04ffff0effff0178ff0580ff808080ff178080ff04ff2cffff04ffff0112ffff04ff80ffff04ffff0bff52ffff0bff3cffff0bff3cff62ff0580ffff0bff3cffff0bff72ffff0bff3cffff0bff3cff62ffff0bffff0101ff0b8080ffff0bff3cff62ff42808080ff42808080ff8080808080ff018080");

pub const XCHANDLES_EXPIRE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    514d248262b0b1607f305a26bf315f6ecb7d7705bfcf5856f12a9a22344af728
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExpireActionArgs {
    pub precommit_1st_curry_hash: Bytes32,
    pub slot_1st_curry_hash: Bytes32,
}

impl XchandlesExpireActionArgs {
    pub fn new(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            precommit_1st_curry_hash: PrecommitLayer::<()>::first_curry_hash(
                SingletonStruct::new(launcher_id).tree_hash().into(),
                relative_block_height,
                payout_puzzle_hash,
            )
            .into(),
            slot_1st_curry_hash: Slot::<()>::first_curry_hash(launcher_id, 0).into(),
        }
    }
}

impl XchandlesExpireActionArgs {
    pub fn curry_tree_hash(
        launcher_id: Bytes32,
        relative_block_height: u32,
        payout_puzzle_hash: Bytes32,
    ) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_EXPIRE_PUZZLE_HASH,
            args: XchandlesExpireActionArgs::new(
                launcher_id,
                relative_block_height,
                payout_puzzle_hash,
            ),
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExpireActionSolution<CMP, CMS, EP, ES, S> {
    pub cat_maker_puzzle_reveal: CMP,
    pub cat_maker_puzzle_solution: CMS,
    pub expired_handle_pricing_puzzle_reveal: EP,
    pub expired_handle_pricing_puzzle_solution: ES,
    pub refund_puzzle_hash_hash: Bytes32,
    pub secret: S,
    pub neighbors: SlotNeigborsInfo,
    pub old_rest: XchandlesDataValue,
    #[clvm(rest)]
    pub new_rest: XchandlesDataValue,
}

pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE: [u8; 333] =
    hex!("ff02ffff01ff04ffff10ffff05ffff02ff05ff81ff8080ffff02ff06ffff04ff02ffff04ffff02ff04ffff04ff02ffff04ff5fffff04ff81bfffff04ffff0101ffff04ffff05ffff14ffff12ffff0183010000ffff3dffff11ff82017fff8202ff80ff0b8080ff0b8080ffff04ffff05ffff14ff17ffff17ffff0101ffff05ffff14ffff11ff82017fff8202ff80ff0b8080808080ff8080808080808080ffff04ff2fff808080808080ffff06ffff02ff05ff81ff808080ffff04ffff01ffff02ffff03ff0bffff01ff02ff04ffff04ff02ffff04ff05ffff04ff1bffff04ffff17ff17ffff010180ffff04ff2fffff04ffff02ffff03ffff18ff2fff1780ffff01ff05ffff14ffff12ff5fff1380ff058080ffff015f80ff0180ff8080808080808080ffff015f80ff0180ff02ffff03ffff15ff05ff0b80ffff01ff11ff05ff0b80ff8080ff0180ff018080");

pub const XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b54c0f4b73e63e78470366bd4006ca629d94f36c8ea58abacf8cc1cbb7724907
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExponentialPremiumRenewPuzzleArgs<P> {
    pub base_program: P,
    pub halving_period: u64,
    pub start_premium: u64,
    pub end_value: u64,
    pub precision: u64,
    pub bits_list: Vec<u64>,
}

pub const PREMIUM_PRECISION: u64 = 1_000_000_000_000_000_000; // 10^18

// https://github.com/ensdomains/ens-contracts/blob/master/contracts/ethregistrar/ExponentialPremiumPriceOracle.sol
pub const PREMIUM_BITS_LIST: [u64; 16] = [
    999989423469314432, // 0.5 ^ 1/65536 * (10 ** 18)
    999978847050491904, // 0.5 ^ 2/65536 * (10 ** 18)
    999957694548431104,
    999915390886613504,
    999830788931929088,
    999661606496243712,
    999323327502650752,
    998647112890970240,
    997296056085470080,
    994599423483633152,
    989228013193975424,
    978572062087700096,
    957603280698573696,
    917004043204671232,
    840896415253714560,
    707106781186547584,
];

impl XchandlesExponentialPremiumRenewPuzzleArgs<NodePtr> {
    pub fn get_start_premium(scale_factor: u64) -> u64 {
        100000000 * scale_factor // start auction at $100 million
    }

    pub fn get_end_value(scale_factor: u64) -> u64 {
        // 100000000 * 10 ** 18 // 2 ** 28 = 372529029846191406
        (372529029846191406_u128 * scale_factor as u128 / 1_000_000_000_000_000_000) as u64
    }

    // A scale factor is how many units of the payment token equate to $1
    // For exampe, you'd use scale_factor=1000 for wUSDC.b
    pub fn from_scale_factor(
        ctx: &mut SpendContext,
        base_price: u64,
        registration_period: u64,
        scale_factor: u64,
    ) -> Result<Self, DriverError> {
        Ok(Self {
            base_program: XchandlesFactorPricingPuzzleArgs::get_puzzle(
                ctx,
                base_price,
                registration_period,
            )?,
            halving_period: 86400, // one day = 86400 = 60 * 60 * 24 seconds
            start_premium: Self::get_start_premium(scale_factor),
            end_value: Self::get_end_value(scale_factor),
            precision: PREMIUM_PRECISION,
            bits_list: PREMIUM_BITS_LIST.to_vec(),
        })
    }

    pub fn curry_tree_hash(
        base_price: u64,
        registration_period: u64,
        scale_factor: u64,
    ) -> TreeHash {
        CurriedProgram {
            program: XCHANDLES_EXPONENTIAL_PREMIUM_RENEW_PUZZLE_HASH,
            args: XchandlesExponentialPremiumRenewPuzzleArgs::<TreeHash> {
                base_program: XchandlesFactorPricingPuzzleArgs::curry_tree_hash(
                    base_price,
                    registration_period,
                ),
                halving_period: 86400, // one day = 86400 = 60 * 60 * 24 seconds
                start_premium: Self::get_start_premium(scale_factor),
                end_value: Self::get_end_value(scale_factor),
                precision: PREMIUM_PRECISION,
                bits_list: PREMIUM_BITS_LIST.to_vec(),
            },
        }
        .tree_hash()
    }

    pub fn get_puzzle(self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.xchandles_exponential_premium_renew_puzzle()?,
            args: self,
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    pub fn get_price(
        self,
        ctx: &mut SpendContext,
        handle: String,
        expiration: u64,
        buy_time: u64,
        num_periods: u64,
    ) -> Result<u128, DriverError> {
        let puzzle = self.get_puzzle(ctx)?;
        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time,
            current_expiration: expiration,
            handle,
            num_periods,
        })?;
        let output = ctx.run(puzzle, solution)?;

        Ok(ctx.extract::<(u128, u64)>(output)?.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
    #[clvm(list)]
    pub struct XchandlesPricingOutput {
        pub price: u128,
        #[clvm(rest)]
        pub registered_time: u64,
    }

    #[test]
    fn test_exponential_premium_puzzle() -> Result<(), DriverError> {
        let mut ctx = SpendContext::new();

        let registration_period = 366 * 24 * 60 * 60;
        let puzzle = XchandlesExponentialPremiumRenewPuzzleArgs::from_scale_factor(
            &mut ctx,
            0,
            registration_period,
            1000,
        )?
        .get_puzzle(&mut ctx)?;

        let mut last_price = 100_000_000_000;
        for day in 0..28 {
            for hour in 0..24 {
                let buy_time = day * 24 * 60 * 60 + hour * 60 * 60;
                let solution = ctx.alloc(&XchandlesPricingSolution {
                    buy_time,
                    current_expiration: 0,
                    handle: "yakuhito".to_string(),
                    num_periods: 1,
                })?;

                let output = ctx.run(puzzle, solution)?;
                let output = ctx.extract::<XchandlesPricingOutput>(output)?;

                assert_eq!(output.registered_time, 366 * 24 * 60 * 60);

                if hour == 0 {
                    let scale_factor =
                        372529029846191406_u128 * 1000_u128 / 1_000_000_000_000_000_000_u128;
                    assert_eq!(
                        output.price,
                        (100_000_000 * 1000) / (1 << day) - scale_factor
                    );
                }

                assert!(output.price < last_price);
                last_price = output.price;

                assert_eq!(
                    XchandlesExponentialPremiumRenewPuzzleArgs::from_scale_factor(
                        &mut ctx,
                        0,
                        registration_period,
                        1000
                    )?
                    .get_price(
                        &mut ctx,
                        "yakuhito".to_string(),
                        0,
                        buy_time,
                        1
                    )?,
                    output.price
                );
            }
        }

        // check premium after auction is 0
        let solution = ctx.alloc(&XchandlesPricingSolution {
            buy_time: 28 * 24 * 60 * 60,
            current_expiration: 0,
            handle: "yakuhito".to_string(),
            num_periods: 1,
        })?;

        let output = ctx.run(puzzle, solution)?;
        let output = ctx.extract::<XchandlesPricingOutput>(output)?;

        assert_eq!(output.registered_time, 366 * 24 * 60 * 60);
        assert_eq!(output.price, 0);

        Ok(())
    }
}
