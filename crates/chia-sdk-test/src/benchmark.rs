use std::{collections::HashMap, fs::File};

use chia_bls::{SecretKey, Signature};
use chia_consensus::spendbundle_conditions::get_conditions_from_spendbundle;
use chia_protocol::{CoinSpend, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::Allocator;
use prettytable::{Table, row};

use crate::Simulator;

/// Collects the CLVM cost of spends, grouped by label, and prints summary statistics.
///
/// # Example
///
/// ```
/// use chia_protocol::CoinSpend;
/// use chia_sdk_test::{Benchmark, Simulator, to_program, to_puzzle};
/// use chia_sdk_types::conditions::{CreateCoin, Memos};
/// use clvmr::{Allocator, NodePtr};
///
/// let mut ctx = Allocator::new();
/// let mut sim = Simulator::new();
/// let mut benchmark = Benchmark::new("Identity puzzle".to_string());
///
/// let (puzzle_hash, puzzle_reveal) = to_puzzle(1)?;
///
/// for _ in 0..3 {
///     let coin = sim.new_coin(puzzle_hash, 1);
///     let solution = to_program([CreateCoin::<NodePtr>::new(puzzle_hash, 1, Memos::None)])?;
///     let coin_spend = CoinSpend::new(coin, puzzle_reveal.clone(), solution);
///     benchmark.add_spends(&mut ctx, &mut sim, vec![coin_spend], "spend", &[])?;
/// }
///
/// assert_eq!(benchmark.data["spend"].len(), 3);
/// benchmark.print_summary(None);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct Benchmark {
    /// The title shown at the top of the summary table.
    pub title: String,
    /// The labels in the order they were first added, which is the row order of the summary.
    pub data_keys: Vec<String>,
    /// The recorded costs for each label.
    pub data: HashMap<String, Vec<u64>>,
}

impl Benchmark {
    /// Creates an empty benchmark with the given title.
    pub fn new(title: String) -> Self {
        Self {
            title,
            data_keys: Vec::new(),
            data: HashMap::new(),
        }
    }

    /// Records the cost of a set of coin spends under the label `key`, then spends them in the
    /// simulator with the given secret keys.
    ///
    /// The cost is the total cost of the spend bundle conditions (CLVM execution plus conditions),
    /// calculated with `ctx` at the simulator's current height. The cost is recorded before the
    /// spend is submitted, even if the spend then fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the conditions can't be calculated, or if
    /// [`Simulator::spend_coins`] fails.
    pub fn add_spends(
        &mut self,
        ctx: &mut Allocator,
        sim: &mut Simulator,
        coin_spends: Vec<CoinSpend>,
        key: &str,
        keys: &[SecretKey],
    ) -> anyhow::Result<()> {
        let sb = SpendBundle::new(coin_spends, Signature::default());
        let sb_conds = get_conditions_from_spendbundle(
            ctx,
            &sb,
            u64::MAX,
            sim.height(),
            &TESTNET11_CONSTANTS,
        )?;

        let key = key.to_string();
        if !self.data_keys.contains(&key) {
            self.data_keys.push(key.clone());
        }
        self.data.entry(key).or_default().push(sb_conds.cost);

        sim.spend_coins(sb.coin_spends, keys)?;
        Ok(())
    }

    /// Prints a table with the average, count, minimum, maximum, and median cost for each label.
    ///
    /// If `filename` is given, the table is also written to that file, replacing its contents.
    ///
    /// # Panics
    ///
    /// Panics if the file can't be created or written.
    #[allow(clippy::cast_precision_loss)]
    pub fn print_summary(&self, filename: Option<&str>) {
        let mut table = Table::new();
        table.add_row(row![format!("Cost statistics for {}", self.title)]);
        table.add_row(row!["label", "avg", "n", "min", "max", "median"]);
        for key in &self.data_keys {
            let data = &self.data[key];

            let total = data.iter().sum::<u64>();
            let avg = format!("{:.1}", total as f64 / data.len() as f64);

            let mut sorted = data.clone();
            sorted.sort_unstable();
            let data_min = sorted[0];
            let data_max = sorted[sorted.len() - 1];

            let data_median = if sorted.len().is_multiple_of(2) {
                (sorted[sorted.len() / 2] + sorted[sorted.len() / 2 - 1]) as f64 / 2.0
            } else {
                sorted[sorted.len() / 2] as f64
            };

            table.add_row(row![key, avg, data.len(), data_min, data_max, data_median]);
        }

        table.printstd();
        if let Some(filename) = filename {
            let mut file = File::create(filename).unwrap();
            table.print(&mut file).unwrap();
        }
    }
}
