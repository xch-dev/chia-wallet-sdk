use chia_protocol::{Bytes, Bytes32, CoinSpend};
use chia_sdk_types::{
    announcement_id,
    conditions::{
        AssertCoinAnnouncement, AssertPuzzleAnnouncement, CreateCoinAnnouncement,
        CreatePuzzleAnnouncement,
    },
    run_puzzle,
};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

/// The announcements created and asserted by a single coin spend.
///
/// This is returned by [`announcements_for_spend`].
#[derive(Debug, Default, Clone)]
pub struct Announcements {
    /// The messages of `CREATE_COIN_ANNOUNCEMENT` conditions.
    pub created_coin: Vec<Bytes>,
    /// The announcement ids of `ASSERT_COIN_ANNOUNCEMENT` conditions.
    pub asserted_coin: Vec<Bytes32>,
    /// The messages of `CREATE_PUZZLE_ANNOUNCEMENT` conditions.
    pub created_puzzle: Vec<Bytes>,
    /// The announcement ids of `ASSERT_PUZZLE_ANNOUNCEMENT` conditions.
    pub asserted_puzzle: Vec<Bytes32>,
}

/// Prints how the announcements created and asserted by a list of coin spends match up.
///
/// For each spend, this prints which spend created each announcement it asserts, and each
/// announcement it creates that appears not to be asserted. Announcement ids and messages are
/// shortened to the hex of their first 4 bytes. This is useful for finding why a spend bundle
/// fails with an announcement error.
///
/// Note that the check for unasserted announcements currently computes the announcement id with
/// the coin id (or puzzle hash) of the asserting spend rather than the creating spend. So an
/// announcement that is created by one spend and asserted by a spend with a different coin id
/// (or puzzle hash) is reported as not asserted, and this function panics.
///
/// # Panics
///
/// Panics if a spend can't be run with [`announcements_for_spend`], or if any announcement is
/// reported as asserted but not created, or created but not asserted.
///
/// # Example
///
/// ```should_panic
/// use chia_protocol::{Bytes32, Coin, CoinSpend};
/// use chia_sdk_test::{debug_announcements, to_program, to_puzzle};
/// use chia_sdk_types::conditions::AssertCoinAnnouncement;
///
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = Coin::new([1; 32].into(), puzzle_hash, 1);
///
/// // No spend creates this announcement, so this prints the problem and panics.
/// let solution = to_program([AssertCoinAnnouncement::new(Bytes32::new([2; 32]))])?;
/// debug_announcements(&[CoinSpend::new(coin, puzzle_reveal, solution)]);
/// # Ok::<(), anyhow::Error>(())
/// ```
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
                println!(
                    "spend {i} asserted unknown coin announcement {}",
                    hex::encode(&asserted_coin[0..4])
                );
                should_panic = true;
                continue;
            };

            println!(
                "spend {i} asserted coin announcement created by spend {created_index}: {}",
                hex::encode(&asserted_coin[0..4])
            );
        }

        for &asserted_puzzle in &announcements.asserted_puzzle {
            let Some(created_index) = all_announcements.iter().enumerate().position(|(i, a)| {
                a.created_puzzle.iter().any(|message| {
                    asserted_puzzle
                        == announcement_id(coin_spends[i].coin.puzzle_hash, message.clone())
                })
            }) else {
                println!(
                    "spend {i} asserted unknown puzzle announcement {}",
                    hex::encode(&asserted_puzzle[0..4])
                );
                should_panic = true;
                continue;
            };

            println!(
                "spend {i} asserted puzzle announcement created by spend {created_index}: {}",
                hex::encode(&asserted_puzzle[0..4])
            );
        }

        for created_coin in &announcements.created_coin {
            let asserted = all_announcements.iter().enumerate().any(|(i, a)| {
                a.asserted_coin.iter().any(|&id| {
                    id == announcement_id(coin_spends[i].coin.coin_id(), created_coin.clone())
                })
            });

            if !asserted {
                println!(
                    "spend {i} created coin announcement {} but it was not asserted",
                    hex::encode(&created_coin[0..4])
                );
                should_panic = true;
            }
        }

        for created_puzzle in &announcements.created_puzzle {
            let asserted = all_announcements.iter().enumerate().any(|(i, a)| {
                a.asserted_puzzle.iter().any(|&id| {
                    id == announcement_id(coin_spends[i].coin.puzzle_hash, created_puzzle.clone())
                })
            });

            if !asserted {
                println!(
                    "spend {i} created puzzle announcement {} but it was not asserted",
                    hex::encode(&created_puzzle[0..4])
                );
                should_panic = true;
            }
        }
    }

    assert!(
        !should_panic,
        "asserted announcements do not match created announcements"
    );
}

/// Runs a coin spend and collects the announcements it creates and asserts.
///
/// Conditions other than coin and puzzle announcements are ignored.
///
/// # Errors
///
/// Returns an error if the puzzle fails to run with the solution, or if its output isn't a list.
///
/// # Example
///
/// ```
/// use chia_protocol::{Coin, CoinSpend};
/// use chia_sdk_test::{announcements_for_spend, to_program, to_puzzle};
/// use chia_sdk_types::conditions::CreatePuzzleAnnouncement;
///
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
/// let coin = Coin::new([0; 32].into(), puzzle_hash, 1);
/// let solution = to_program([CreatePuzzleAnnouncement::new(b"hi".to_vec().into())])?;
///
/// let announcements = announcements_for_spend(&CoinSpend::new(coin, puzzle_reveal, solution))?;
/// assert_eq!(announcements.created_puzzle, vec![b"hi".to_vec().into()]);
/// assert!(announcements.created_coin.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn announcements_for_spend(coin_spend: &CoinSpend) -> anyhow::Result<Announcements> {
    let mut announcements = Announcements::default();

    let allocator = &mut Allocator::new();
    let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
    let solution = coin_spend.solution.to_clvm(allocator)?;

    let output = run_puzzle(allocator, puzzle, solution)?;

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
