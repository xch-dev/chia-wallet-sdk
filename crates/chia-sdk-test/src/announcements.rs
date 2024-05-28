use chia_protocol::{Bytes, Bytes32, CoinSpend};
use chia_sdk_types::conditions::{
    AssertCoinAnnouncement, AssertPuzzleAnnouncement, CreateCoinAnnouncement,
    CreatePuzzleAnnouncement,
};
use clvm_traits::{FromClvm, ToNodePtr};
use clvmr::{
    reduction::Reduction,
    run_program,
    sha2::{Digest, Sha256},
    Allocator, ChiaDialect, NodePtr,
};

#[derive(Debug, Default, Clone)]
pub struct Announcements {
    pub created_coin: Vec<Bytes>,
    pub asserted_coin: Vec<Bytes32>,
    pub created_puzzle: Vec<Bytes>,
    pub asserted_puzzle: Vec<Bytes32>,
}

/// Print the announcements that are created and asserted by a list of coin spends.
///
/// # Panics
///
/// Panics if the announcements cannot be extracted from the coin spends.
pub fn debug_announcements(coin_spends: &[CoinSpend]) {
    let all_announcements: Vec<Announcements> = coin_spends
        .iter()
        .map(|coin_spend| {
            announcements_for_spend(coin_spend).expect("could not extract announcements")
        })
        .collect();

    let mut should_panic = false;

    for (i, announcements) in all_announcements.iter().enumerate() {
        for &asserted_coin in &announcements.asserted_coin {
            let Some(created_index) = all_announcements.iter().enumerate().position(|(i, a)| {
                a.created_coin.iter().any(|message| {
                    asserted_coin == announcement_id(coin_spends[i].coin.coin_id(), message.clone())
                })
            }) else {
                println!("spend at index {i} asserted unknown coin announcement");
                should_panic = true;
                continue;
            };

            println!(
                "spend at index {i} asserted coin announcement created by spend at index {created_index}"
            );
        }

        for &asserted_puzzle in &announcements.asserted_puzzle {
            let Some(created_index) = all_announcements.iter().enumerate().position(|(i, a)| {
                a.created_puzzle.iter().any(|message| {
                    asserted_puzzle
                        == announcement_id(coin_spends[i].coin.puzzle_hash, message.clone())
                })
            }) else {
                println!("spend at index {i} asserted unknown puzzle announcement");
                should_panic = true;
                continue;
            };

            println!(
                "spend at index {i} asserted puzzle announcement created by spend at index {created_index}"
            );
        }
    }

    assert!(
        !should_panic,
        "asserted announcements do not match created announcements"
    );
}

pub fn announcement_id(id: Bytes32, message: Bytes) -> Bytes32 {
    let mut hasher = Sha256::new();
    hasher.update(id);
    hasher.update(message);
    Bytes32::new(hasher.finalize().into())
}

pub fn announcements_for_spend(coin_spend: &CoinSpend) -> anyhow::Result<Announcements> {
    let mut announcements = Announcements::default();

    let allocator = &mut Allocator::new();
    let puzzle = coin_spend.puzzle_reveal.to_node_ptr(allocator)?;
    let solution = coin_spend.solution.to_node_ptr(allocator)?;

    let Reduction(_cost, output) = run_program(
        allocator,
        &ChiaDialect::new(0),
        puzzle,
        solution,
        11_000_000_000,
    )?;

    let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;

    for condition in conditions {
        if let Ok(condition) = CreateCoinAnnouncement::from_clvm(allocator, condition) {
            announcements.created_coin.push(condition.message);
        } else if let Ok(condition) = CreatePuzzleAnnouncement::from_clvm(allocator, condition) {
            announcements.created_puzzle.push(condition.message);
        } else if let Ok(condition) = AssertCoinAnnouncement::from_clvm(allocator, condition) {
            announcements.asserted_coin.push(condition.announcement_id);
        } else if let Ok(condition) = AssertPuzzleAnnouncement::from_clvm(allocator, condition) {
            announcements
                .asserted_puzzle
                .push(condition.announcement_id);
        }
    }

    Ok(announcements)
}
