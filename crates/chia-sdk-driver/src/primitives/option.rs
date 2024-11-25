use chia_protocol::{Bytes32, Coin};
use chia_puzzles::offer::{NotarizedPayment, Payment, SETTLEMENT_PAYMENTS_PUZZLE_HASH};
use chia_sdk_types::{announcement_id, AssertPuzzleAnnouncement, Conditions};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::Allocator;
use hex_literal::hex;

use crate::{DriverError, Layer, Make, Offer, OfferBuilder, P2ConditionsOptionsArgs, SpendContext};

use super::NftInfo;

// // parsed from the options in p2_conditions_options_layer
// struct OptionContractInfo {
//     assertion: AssertPuzzleAnnouncement,
//     before_timestamp: u64,
//     after_timestamp: u64,
// }

// impl OptionContractInfo {
//     pub fn matches_nft_info(&self, nft_info: &NftInfo) -> bool {
//         let assertion = payment_assertion(
//             nft_puzzle_hash.into(),
//             &NotarizedPayment {
//                 nonce: nft_info.launcher_id,
//                 payments: vec![Payment::with_memos(
//                     BURN_PUZZLE_HASH,
//                     1,
//                     vec![BURN_PUZZLE_HASH.into()],
//                 )],
//             },
//         );

//         self.assertion == assertion
//     }
// }

// impl P2ConditionsOptionsLayer {
//     pub fn to_option_contract_info(&self) -> Option<OptionContractInfo> {
//         // if it matches the standard format return it
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Option {
    pub coin: Coin,
    pub expiration_height: u32,
    pub contract_id: Bytes32,
    pub requested_p2: Bytes32,
    pub offered_amount: u64,
}

impl Option {
    pub fn new(
        coin: Coin,
        expiration_height: u32,
        contract_id: Bytes32,
        requested_p2: Bytes32,
        offered_amount: u64,
    ) -> Self {
        Self {
            coin,
            expiration_height,
            contract_id,
            requested_p2,
            offered_amount,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionContract<M> {
    pub nft_info: NftInfo<M>,
    pub p2_puzzle_hash: Bytes32,
}

impl<M> OptionContract<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    /// Creates the p2 option puzzle hash, used to lock up the option coins.
    pub fn p2_option_puzzle(
        &self,
        ctx: &mut SpendContext,
        expiration_height: u32,
        offered_amount: u64,
        assertions: Conditions,
        include_hint: bool,
    ) -> Result<P2ConditionsOptionsArgs, DriverError> {
        let settlement_payments = ctx.settlement_payments_puzzle()?;
        let nft_puzzle = self
            .nft_info
            .clone()
            .into_layers(settlement_payments)
            .construct_puzzle(ctx)?;
        let nft_puzzle_hash = ctx.tree_hash(nft_puzzle);

        let burn_nft_assertion = payment_assertion(
            nft_puzzle_hash.into(),
            &NotarizedPayment {
                nonce: self.nft_info.launcher_id,
                payments: vec![Payment::with_memos(
                    BURN_PUZZLE_HASH,
                    1,
                    vec![BURN_PUZZLE_HASH.into()],
                )],
            },
        );

        let pre_conditions = Conditions::new()
            .assert_before_height_absolute(expiration_height)
            .create_coin(SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), offered_amount, None)
            .with(burn_nft_assertion)
            .extend(assertions);

        let post_conditions = Conditions::new()
            .assert_height_absolute(expiration_height)
            .create_coin(
                self.p2_puzzle_hash,
                offered_amount,
                if include_hint {
                    Some(ctx.hint(self.p2_puzzle_hash)?)
                } else {
                    None
                },
            );

        Ok(P2ConditionsOptionsArgs::new(vec![
            pre_conditions,
            post_conditions,
        ]))
    }

    pub fn make_offer(
        &self,
        ctx: &mut SpendContext,
        offered_coin_id: Bytes32,
    ) -> Result<OfferBuilder<Make>, DriverError> {
        let nonce = Offer::nonce(vec![offered_coin_id]);
        let builder = OfferBuilder::new(nonce);

        let settlement_payments = ctx.settlement_payments_puzzle()?;
        let nft_puzzle = self
            .nft_info
            .clone()
            .into_layers(settlement_payments)
            .construct_puzzle(ctx)?;

        builder.request(ctx, &nft_puzzle, vec![Payment::new(BURN_PUZZLE_HASH, 1)])
    }
}

pub fn payment_assertion(
    puzzle_hash: Bytes32,
    notarized_payment: &NotarizedPayment,
) -> AssertPuzzleAnnouncement {
    let mut allocator = Allocator::new();
    let notarized_payment_ptr = notarized_payment.to_clvm(&mut allocator).unwrap();
    let notarized_payment_hash = tree_hash(&allocator, notarized_payment_ptr);
    AssertPuzzleAnnouncement::new(announcement_id(puzzle_hash, notarized_payment_hash))
}

const BURN_PUZZLE_HASH: Bytes32 = Bytes32::new(hex!(
    "000000000000000000000000000000000000000000000000000000000000dead"
));

#[cfg(test)]
mod tests {
    use chia_puzzles::nft::NftMetadata;
    use chia_sdk_test::Simulator;

    use crate::{IntermediateLauncher, Launcher, NftMint, StandardLayer};

    use super::*;

    #[test]
    fn test_option_contract() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();
        let (sk, pk, p2_puzzle_hash, coin) = sim.new_p2(1002)?;
        let p2 = StandardLayer::new(pk);

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, &p2)?;
        p2.spend(ctx, coin, create_did)?;

        let mint = NftMint::new(NftMetadata::default(), p2_puzzle_hash, 0, None);

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, mint)?;
        let _did = did.update(ctx, &p2, mint_nft)?;
        let _nft = nft.transfer(ctx, &p2, p2_puzzle_hash, Conditions::new())?;

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }
}

/*
async def create_option(
    self,
    request: Dict[str, Any],
    action_scope: WalletActionScope,
    extra_conditions: Tuple[Condition, ...] = tuple(),
) -> EndpointResult:
    def make_option_puzzle(
        *,
        expiration_height,
        contract_id: bytes32,
        contract_puzzle_info: PuzzleInfo,
        requested_p2: bytes32,
        requested_amount,
        offered_amount,
        requested_asset_id: Optional[bytes32] = None,
        offered_asset_id: Optional[bytes32] = None,
    ):
        coins = [Coin(contract_id, contract_id, uint64(1))]
        memos = [requested_p2]

        # pre conditions
        payments: Dict[Optional[bytes32], List[Payment]] = {}
        pre_drivers = {}
        if requested_asset_id is not None:
            pre_drivers[requested_asset_id] = PuzzleInfo({"type": AssetType.CAT.value, "tail": "0x" + requested_asset_id.hex()})
        payments[requested_asset_id] = [Payment(requested_p2, uint64(requested_amount), memos)]

        # the contract should be burned
        pre_drivers[contract_id] = contract_puzzle_info
        payments[contract_id] = [Payment(hexstr_to_bytes("0x000000000000000000000000000000000000000000000000000000000000dead"), uint64(1), [])]

        # taker payments for contract execution
        pre_notarized_payments: Dict[Optional[bytes32], List[NotarizedPayment]] = Offer.notarize_payments(payments, coins)
        pre_announcements_to_assert: List[AssertPuzzleAnnouncement] = Offer.calculate_announcements(pre_notarized_payments, pre_drivers)
        pre_conditions = [
            [ConditionOpcode.ASSERT_BEFORE_HEIGHT_ABSOLUTE, expiration_height],
            [ConditionOpcode.CREATE_COIN, OFFER_MOD.get_tree_hash(), offered_amount],
            *(make_assert_puzzle_announcement(a.msg_calc) for a in pre_announcements_to_assert)
        ]


        # post conditions, send back to the requested_p2
        payments: Dict[Optional[bytes32], List[Payment]] = {}
        post_drivers = {}
        if offered_asset_id is not None:
            post_drivers[offered_asset_id] = PuzzleInfo({"type": AssetType.CAT.value, "tail": "0x" + offered_asset_id.hex()})
        payments[offered_asset_id] = [Payment(requested_p2, uint64(offered_amount-1), memos)] # why is this -1
        post_conditions = [
            [ConditionOpcode.ASSERT_HEIGHT_ABSOLUTE, expiration_height],
            [ConditionOpcode.CREATE_COIN, requested_p2, offered_amount, memos],
        ]

        # make puzzle
        conditions_options = [pre_conditions, post_conditions]
        puzzle = puzzle_for_conditions_options(
            conditions_options
        )

        # make solutions
        pre_solution = solution_for_conditions_options(0)
        post_solution = solution_for_conditions_options(1)

        return puzzle, pre_solution, post_solution, pre_notarized_payments, pre_drivers

    def make_option_puzzle_solutions(
        *,
        p2_contract: Program,
        coin: Coin,
        offered_asset_id: Optional[bytes32] = None,
        lineage_proof: Optional[Program] = None,
        pre_solution: Program,
        post_solution: Program,
    ):
        if offered_asset_id is not None:
            p2_contract_ph = p2_contract.get_tree_hash()
            puzzle_reveal = construct_cat_puzzle(CAT_MOD, offered_asset_id, p2_contract)
            pre_solution = Program.to(
                [
                    pre_solution, # inner puzzle solution
                    lineage_proof,
                    coin.name(),
                    coin_as_list(coin),
                    [coin.parent_coin_info, p2_contract_ph, coin.amount],
                    0,
                    0,
                ]
            )
            post_solution = Program.to(
                [
                    post_solution, # inner puzzle solution
                    lineage_proof,
                    coin.name(),
                    coin_as_list(coin),
                    [coin.parent_coin_info, p2_contract_ph, coin.amount],
                    0,
                    0,
                ]
            )
        else:
            puzzle_reveal = p2_contract

        return puzzle_reveal, pre_solution, post_solution

    # make offer file to execute contract
    def make_option_execute_offer(
        *,
        coin: Coin,
        puzzle: Program,
        solution: Program,
        notarized_payments,
        drivers,
    ) -> Offer:
        coin_spend = CoinSpend(coin, SerializedProgram.from_program(puzzle), SerializedProgram.from_program(solution))
        return Offer(notarized_payments, SpendBundle([coin_spend], G2Element()), drivers)


    # make spend bundle to return contract coin
    def make_option_cancel_spend(
        *,
        coin: Coin,
        puzzle: Program,
        solution: Program
    ) -> SpendBundle:
        coin_spend = CoinSpend(coin, SerializedProgram.from_program(puzzle), SerializedProgram.from_program(solution))
        return SpendBundle([coin_spend], G2Element())


    # wallet id of a nft wallet to look up contract coin
    nft_wallet = self.service.wallet_state_manager.get_wallet(id=uint32(request["wallet_id"]), required_type=NFTWallet)
    # coin id for contract nft
    contract_puzzle = await nft_wallet.get_nft_coin_by_id(bytes32.from_hexstr(request["contract_nft_coin_id"]))
    contract_puzzle_info: Optional[PuzzleInfo] = match_puzzle(uncurry_puzzle(contract_puzzle.full_puzzle))
    # requested payment address for contract execution or return after expiration
    requested_p2 = bytes32.from_hexstr(request["requested_p2"])
    # requested amount for seller
    requested_amount = uint32(request["requested_amount"])
    # requested asset id (0 for xch)
    requested_asset_id = bytes32.from_hexstr(request["requested_asset_id"])
    # asset id seller is offering and locking up (0 for xch)
    offered_asset_id = bytes32.from_hexstr(request["offered_asset_id"])
    # amount seller is offering and locking up
    offered_amount = uint32(request["offered_amount"])
    expiration_height: Any = uint32(request["expiration_height"])
    contract_id = create_asset_id(contract_puzzle_info)
    funded_coin_parent_id = None
    execution_option_offer = None
    cancel_option_spend = None

    if offered_asset_id == bytes32.from_hexstr("0x0000000000000000000000000000000000000000000000000000000000000000"):
        offered_asset_id = None

    if requested_asset_id == bytes32.from_hexstr("0x0000000000000000000000000000000000000000000000000000000000000000"):
        requested_asset_id = None

    p2_contract, pre_solution, post_solution, notarized_payments, drivers = make_option_puzzle(
        expiration_height=expiration_height,
        contract_id=contract_id,
        contract_puzzle_info=contract_puzzle_info,
        requested_p2=requested_p2,
        requested_amount=requested_amount,
        offered_amount=offered_amount,
        requested_asset_id=requested_asset_id,
        offered_asset_id=offered_asset_id,
    )
    p2_contract_ph = p2_contract.get_tree_hash()

    if "funded_coin_parent_id" in request:
        funded_coin_parent_id = bytes32.from_hexstr(request["funded_coin_parent_id"])
        funded_coin = Coin(funded_coin_parent_id, p2_contract_ph, uint64(offered_amount))

        lineage_proof = None
        if offered_asset_id != None:
            funded_coin = Coin(funded_coin_parent_id, construct_cat_puzzle(CAT_MOD, offered_asset_id, p2_contract).get_tree_hash(), uint64(offered_amount))
            # log.error("funded cat coin")
            # log.error(funded_coin)

            wallet_state_manager = self.service.wallet_state_manager
            if await self.service.wallet_state_manager.synced() is False:
                raise ValueError("Wallet needs to be fully synced.")
            main_wallet = wallet_state_manager.main_wallet
            cat_wallet = await CATWallet.get_or_create_wallet_for_cat(
                wallet_state_manager, main_wallet, request["offered_asset_id"], request["offered_asset_id"]
            )
            lineage_proof = (await cat_wallet.get_lineage_proof_for_coin(funded_coin)).to_program()

        p2_contract_reveal, p2_contract_solution, p2_contract_solution_post = make_option_puzzle_solutions(
            p2_contract=p2_contract,
            coin=funded_coin,
            offered_asset_id=offered_asset_id,
            lineage_proof=lineage_proof,
            pre_solution=pre_solution,
            post_solution=post_solution,
        )

        # log.error("coin")
        # log.error(type(funded_coin))
        # log.error("p2_contract_reveal")
        # log.error(type(p2_contract_reveal))
        # log.error("p2_contract_solution")
        # log.error(type(p2_contract_solution))

        execution_option_offer = make_option_execute_offer(coin=funded_coin, puzzle=p2_contract_reveal, solution=p2_contract_solution, notarized_payments=notarized_payments, drivers=drivers).to_bech32()
        cancel_option_spend = make_option_cancel_spend(coin=funded_coin, puzzle=p2_contract_reveal, solution=p2_contract_solution_post)


    return {
        "p2_contract": p2_contract,
        "p2_contract_ph": p2_contract_ph,
        "expiration_height": expiration_height,
        "contract_id": contract_id,
        "requested_p2": requested_p2,
        "requested_amount": requested_amount,
        "requested_asset_id": requested_asset_id,
        "offered_amount": offered_amount,
        "offered_asset_id": offered_asset_id,
        "execution_option_offer": execution_option_offer,
        "cancel_option_spend": cancel_option_spend,
    }


*/
