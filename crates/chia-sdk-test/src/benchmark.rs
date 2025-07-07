use std::{collections::HashMap, fs::File};

use chia_bls::{SecretKey, Signature};
use chia_consensus::spendbundle_conditions::get_conditions_from_spendbundle;
use chia_protocol::{CoinSpend, SpendBundle};
use chia_sdk_types::TESTNET11_CONSTANTS;
use clvmr::Allocator;
use prettytable::{row, Table};

use crate::Simulator;

#[derive(Debug, Clone)]
pub struct Benchmark {
    pub title: String,
    pub data_keys: Vec<String>,
    pub data: HashMap<String, Vec<u64>>,
}

impl Benchmark {
    pub fn new(title: String) -> Self {
        Self {
            title,
            data_keys: Vec::new(),
            data: HashMap::new(),
        }
    }

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

            let data_median = if sorted.len() % 2 == 0 {
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
