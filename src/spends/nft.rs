use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_wallet::{
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    singleton::SingletonSolution,
    standard::StandardSolution,
    Proof,
};
use clvm_traits::{clvm_quote, ToClvm};
use clvmr::NodePtr;

use crate::{NewNftOwner, SpendContext, SpendError};

/// The new DID owner of the NFT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NftOwner {
    /// The DID id of the new owner.
    did_id: Bytes32,

    /// The DID inner puzzle hash of the new owner.
    did_inner_puzzle_hash: Bytes32,
}

/// Constructs the ownership transfer condition.
pub fn transfer_ownership(new_owner: Option<NftOwner>) -> NewNftOwner {
    match new_owner {
        Some(NftOwner {
            did_id,
            did_inner_puzzle_hash,
        }) => NewNftOwner {
            new_owner: Some(did_id),
            trade_prices_list: Vec::new(),
            new_did_inner_hash: Some(did_inner_puzzle_hash),
        },
        None => NewNftOwner {
            new_owner: None,
            trade_prices_list: Vec::new(),
            new_did_inner_hash: None,
        },
    }
}

/// Spend an NFT.
pub fn spend_nft<T>(
    ctx: &mut SpendContext,
    coin: Coin,
    puzzle_reveal: Program,
    proof: Proof,
    conditions: T,
) -> Result<CoinSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    // Construct the p2 solution.
    let p2_solution = StandardSolution {
        original_public_key: None,
        delegated_puzzle: clvm_quote!(conditions),
        solution: (),
    };

    // Construct the ownership layer solution.
    let ownership_layer_solution = NftOwnershipLayerSolution {
        inner_solution: p2_solution,
    };

    // Construct the state layer solution.
    let state_layer_solution = NftStateLayerSolution {
        inner_solution: ownership_layer_solution,
    };

    // Construct the singleton solution.
    let solution = ctx.serialize(SingletonSolution {
        proof,
        amount: coin.amount,
        inner_solution: state_layer_solution,
    })?;

    // Construct the coin spend.
    let coin_spend = CoinSpend::new(coin, puzzle_reveal, solution);

    Ok(coin_spend)
}

// pub fn mint_nfts(
//     a: &mut Allocator,
//     did_info: DidInfo,
//     nft_mints: &[NftMint],
//     start_index: usize,
//     total_nft_count: usize,
//     fee: u64,
// ) -> Result<(Vec<CoinSpend>, Vec<[u8; 32]>)> {
//     // Get DID info.
//     let did_info = self
//         .state
//         .read()
//         .await
//         .get_did_info(did_id)
//         .ok_or(anyhow::Error::msg("could not find DID info"))?;

//     // Select coins and calculate amounts.
//     let nft_amount = 1;
//     let required_amount = nft_mints.len() as u64 * nft_amount + fee;
//     let selected_coins = self
//         .state
//         .read()
//         .await
//         .select_standard_coins(required_amount);
//     let funding_coin = selected_coins
//         .first()
//         .ok_or(anyhow::Error::msg("no funding coin"))?;

//     // Initialize the allocator and puzzles.
//     let intermediate_launcher_mod = node_from_bytes(&mut a, &NFT_INTERMEDIATE_LAUNCHER_PUZZLE)?;
//     let transfer_program_mod = node_from_bytes(&mut a, &NFT_ROYALTY_TRANSFER_PUZZLE)?;
//     let ownership_layer_mod = node_from_bytes(&mut a, &NFT_OWNERSHIP_LAYER_PUZZLE)?;
//     let state_layer_mod = node_from_bytes(&mut a, &NFT_STATE_LAYER_PUZZLE)?;
//     let singleton_mod = node_from_bytes(&mut a, &SINGLETON_PUZZLE)?;
//     let p2_mod = node_from_bytes(&mut a, &STANDARD_PUZZLE)?;

//     // Construct the p2 puzzle.
//     let p2_puzzle_hash = self.state.write().await.unused_puzzle_hash().await?;

//     let p2_args = StandardArgs {
//         synthetic_key: self
//             .state
//             .read()
//             .await
//             .key_store
//             .secret_key_of(&p2_puzzle_hash)
//             .ok_or(anyhow::Error::msg("missing secret key for p2 spend"))?
//             .to_public_key(),
//     }
//     .to_clvm(&mut a)?;
//     let p2 = curry(&mut a, p2_mod, p2_args)?;

//     // Collect spend information for each NFT mint.
//     let mut coin_spends = Vec::new();
//     let mut did_condition_list = Vec::new();
//     let mut signatures = Vec::new();
//     let mut nft_ids = Vec::new();

//     // Prepare NFT mint spends.
//     for (raw_index, nft_mint) in nft_mints.iter().enumerate() {
//         let index = start_index + raw_index;

//         // Create intermediate launcher to prevent launcher id collisions.
//         let intermediate_args = NftIntermediateLauncherArgs {
//             launcher_puzzle_hash: LAUNCHER_PUZZLE_HASH,
//             mint_number: index,
//             mint_total: total_nft_count,
//         }
//         .to_clvm(&mut a)?;
//         let intermediate_puzzle = curry(&mut a, intermediate_launcher_mod, intermediate_args)?;
//         let intermediate_puzzle_hash = tree_hash(&a, intermediate_puzzle);

//         let intermediate_coin = Coin::new(
//             did_info.coin_state.coin.coin_id().into(),
//             intermediate_puzzle_hash.into(),
//             0,
//         );

//         let intermediate_coin_id = intermediate_coin.coin_id();

//         did_condition_list.push(Condition::CreateCoin {
//             puzzle_hash: intermediate_puzzle_hash,
//             amount: 0,
//             memos: vec![],
//         });

//         // Spend intermediate launcher.
//         let intermediate_solution = a.null();

//         let intermediate_spend = CoinSpend::new(
//             intermediate_coin.clone(),
//             Program::from_clvm(&a, intermediate_puzzle)?,
//             Program::from_clvm(&a, intermediate_solution)?,
//         );

//         coin_spends.push(intermediate_spend);

//         // Assert intermediate launcher info in DID spend.
//         let mut hasher = Sha256::new();
//         hasher.update(int_to_bytes(index.into()));
//         hasher.update(int_to_bytes(total_nft_count.into()));
//         let announcement_message: [u8; 32] = hasher.finalize_fixed().into();

//         let mut hasher = Sha256::new();
//         hasher.update(intermediate_coin_id);
//         hasher.update(announcement_message);
//         let announcement_id: [u8; 32] = hasher.finalize_fixed().into();

//         did_condition_list.push(Condition::AssertCoinAnnouncement { announcement_id });

//         // Create the launcher coin.
//         let launcher_coin = Coin::new(
//             intermediate_coin_id.into(),
//             LAUNCHER_PUZZLE_HASH.into(),
//             nft_amount,
//         );
//         let launcher_id = launcher_coin.coin_id();

//         nft_ids.push(launcher_id);

//         did_condition_list.push(Condition::CreatePuzzleAnnouncement {
//             message: launcher_id,
//         });

//         let nft_singleton_struct = SingletonStruct::from_launcher_id(launcher_id);

//         // Curry the NFT ownership layer for the eve coin.
//         let eve_transfer_program_args = NftRoyaltyTransferPuzzleArgs {
//             singleton_struct: nft_singleton_struct.clone(),
//             royalty_puzzle_hash: nft_mint.royalty_puzzle_hash,
//             trade_price_percentage: nft_mint.royalty_percentage,
//         }
//         .to_clvm(&mut a)?;

//         let eve_transfer_program = curry(&mut a, transfer_program_mod, eve_transfer_program_args)?;

//         let eve_ownership_layer_args = NftOwnershipLayerArgs {
//             mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
//             current_owner: None,
//             transfer_program: LazyNode(eve_transfer_program),
//             inner_puzzle: LazyNode(p2),
//         }
//         .to_clvm(&mut a)?;

//         let eve_ownership_layer = curry(&mut a, ownership_layer_mod, eve_ownership_layer_args)?;

//         // Curry the NFT state layer for the eve coin.
//         let metadata = nft_mint.metadata.to_clvm(&mut a)?;

//         let eve_state_layer_args = NftStateLayerArgs {
//             mod_hash: NFT_STATE_LAYER_PUZZLE_HASH,
//             metadata: LazyNode(metadata),
//             metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_PUZZLE_HASH,
//             inner_puzzle: LazyNode(eve_ownership_layer),
//         }
//         .to_clvm(&mut a)?;

//         let eve_state_layer = curry(&mut a, state_layer_mod, eve_state_layer_args)?;

//         // Curry the singleton for the eve coin.
//         let eve_singleton_args = SingletonArgs {
//             singleton_struct: nft_singleton_struct,
//             inner_puzzle: LazyNode(eve_state_layer),
//         }
//         .to_clvm(&mut a)?;

//         let eve_singleton = curry(&mut a, singleton_mod, eve_singleton_args)?;
//         let eve_puzzle_hash = tree_hash(&a, eve_singleton);

//         // The DID spend will assert an announcement from the eve coin.
//         let announcement_message_content =
//             clvm_list!(eve_puzzle_hash, nft_amount, ()).to_clvm(&mut a)?;
//         let announcement_message = tree_hash(&a, announcement_message_content);

//         let mut hasher = Sha256::new();
//         hasher.update(launcher_id);
//         hasher.update(announcement_message);
//         let announcement_id: [u8; 32] = hasher.finalize_fixed().into();

//         did_condition_list.push(Condition::AssertCoinAnnouncement { announcement_id });

//         // Spend the launcher coin.
//         let launcher_solution = LauncherSolution {
//             singleton_puzzle_hash: eve_puzzle_hash,
//             amount: nft_amount,
//             key_value_list: LazyNode(a.null()),
//         }
//         .to_clvm(&mut a)?;

//         let launcher_spend = CoinSpend::new(
//             launcher_coin.clone(),
//             Program::parse(&mut Cursor::new(&LAUNCHER_PUZZLE))?,
//             Program::from_clvm(&a, launcher_solution)?,
//         );

//         coin_spends.push(launcher_spend);

//         // Create the eve coin info.
//         let eve_coin = Coin::new(
//             launcher_coin.coin_id().into(),
//             eve_puzzle_hash.into(),
//             nft_amount,
//         );

//         let eve_proof = EveProof {
//             parent_coin_info: intermediate_coin.coin_id(),
//             amount: nft_amount,
//         };

//         self.state.write().await.update_nft(NftInfo {
//             launcher_id,
//             puzzle_reveal: Program::from_clvm(&a, eve_singleton)?,
//             p2_puzzle_hash,
//             coin_state: CoinState::new(eve_coin, None, None),
//             proof: Proof::Eve(eve_proof),
//         })?;

//         // Create eve coin spend.
//         let eve_spend_conditions = vec![Condition::CreateCoin {
//             puzzle_hash: nft_mint.target_puzzle_hash,
//             amount: nft_amount as i64,
//             memos: vec![nft_mint.target_puzzle_hash],
//         }];

//         let (eve_coin_spend, signature, announcement_message) = self
//             .spend_nft(
//                 &launcher_id,
//                 NewOwner::DidInfo {
//                     did_id: did_info.launcher_id,
//                     did_inner_puzzle_hash: did_info.inner_puzzle_hash,
//                 },
//                 eve_spend_conditions,
//             )
//             .await?;

//         coin_spends.push(eve_coin_spend);
//         signatures.push(signature);

//         // Assert eve puzzle announcement in funding spend.
//         let mut hasher = Sha256::new();
//         hasher.update(eve_puzzle_hash);
//         hasher.update(announcement_message.unwrap());
//         let announcement_id: [u8; 32] = hasher.finalize_fixed().into();

//         did_condition_list.push(Condition::AssertPuzzleAnnouncement { announcement_id });
//     }

//     // Calculate change.
//     let spent_amount = selected_coins
//         .iter()
//         .fold(0, |amount, coin| amount + coin.amount);
//     let change_amount = spent_amount - required_amount;
//     let change_puzzle_hash = self.state.write().await.unused_puzzle_hash().await?;

//     // Calculate announcement message.
//     let mut hasher = Sha256::new();
//     selected_coins
//         .iter()
//         .for_each(|coin| hasher.update(coin.coin_id()));
//     if change_amount > 0 {
//         hasher.update(
//             Coin::new(
//                 funding_coin.coin_id().into(),
//                 change_puzzle_hash.into(),
//                 change_amount,
//             )
//             .coin_id(),
//         );
//     }

//     let announcement_message: [u8; 32] = hasher.finalize_fixed().into();

//     did_condition_list.push(Condition::CreateCoinAnnouncement {
//         message: announcement_message,
//     });

//     // Calculate primary announcement id.
//     let mut hasher = Sha256::new();
//     hasher.update(funding_coin.coin_id());
//     hasher.update(announcement_message);
//     let primary_announcement_id: [u8; 32] = hasher.finalize_fixed().into();

//     // Spend standard coins.
//     for (index, coin) in selected_coins.iter().enumerate() {
//         // Fetch the key pair.
//         let secret_key = self
//             .state
//             .read()
//             .await
//             .key_store
//             .secret_key_of((&coin.puzzle_hash).into())
//             .ok_or(anyhow::Error::msg("missing secret key for fee coin spend"))?
//             .clone();
//         let public_key = secret_key.to_public_key();

//         // Construct the p2 puzzle.
//         let fee_p2_args = StandardArgs {
//             synthetic_key: public_key,
//         }
//         .to_clvm(&mut a)?;
//         let fee_p2 = curry(&mut a, p2_mod, fee_p2_args)?;

//         // Calculate the conditions.
//         let condition_list = if index == 0 {
//             let mut condition_list = vec![];

//             // Announce to other coins.
//             if selected_coins.len() > 1 {
//                 condition_list.push(Condition::CreateCoinAnnouncement {
//                     message: announcement_message,
//                 });
//             }

//             // Assert DID announcement.
//             let mut hasher = Sha256::new();
//             hasher.update(did_info.coin_state.coin.coin_id());
//             hasher.update(announcement_message);
//             let did_announcement_id: [u8; 32] = hasher.finalize_fixed().into();

//             condition_list.push(Condition::AssertCoinAnnouncement {
//                 announcement_id: did_announcement_id,
//             });

//             // Create change coin.
//             if change_amount > 0 {
//                 condition_list.push(Condition::CreateCoin {
//                     puzzle_hash: change_puzzle_hash,
//                     amount: change_amount as i64,
//                     memos: vec![],
//                 });
//             }

//             condition_list
//         } else {
//             vec![Condition::AssertCoinAnnouncement {
//                 announcement_id: primary_announcement_id,
//             }]
//         };

//         let conditions = clvm_quote!(condition_list).to_clvm(&mut a)?;
//         let conditions_tree_hash = tree_hash(&a, conditions);
//         let solution = StandardSolution::with_conditions(&mut a, conditions).to_clvm(&mut a)?;

//         // Create the coin spend.
//         let coin_spend = CoinSpend::new(
//             coin.clone(),
//             Program::from_clvm(&a, fee_p2)?,
//             Program::from_clvm(&a, solution)?,
//         );

//         coin_spends.push(coin_spend);
//     }

//     let (did_message_spend, did_signature) = self
//         .spend_did(
//             did_id,
//             did_info.inner_puzzle_hash,
//             did_info.p2_puzzle_hash,
//             did_condition_list,
//         )
//         .await?;

//     coin_spends.push(did_message_spend);

//     Ok((coin_spends, nft_ids))
// }
