// https://raw.githubusercontent.com/Datalayer-Storage/DataLayer-Driver/1aafc1e89734ddd6d3bda98d9ff45709507469ea/src/debug.rs

use std::{fs::File, io::Write, path::Path};

use chia_bls::G2Element;
use chia_protocol::{Coin, CoinSpend, SpendBundle};
use hex::encode;
use serde::Serialize;

#[derive(Serialize)]
struct SerializableCoin {
    parent_coin_info: String,
    puzzle_hash: String,
    amount: u64,
}

#[derive(Serialize)]
struct SerializableCoinSpend {
    coin: SerializableCoin,
    puzzle_reveal: String,
    solution: String,
}

#[derive(Serialize)]
struct SerializableSpendBundle {
    coin_spends: Vec<SerializableCoinSpend>,
    aggregated_signature: String,
}

impl From<&Coin> for SerializableCoin {
    fn from(coin: &Coin) -> Self {
        SerializableCoin {
            parent_coin_info: format!("0x{}", encode(coin.parent_coin_info)),
            puzzle_hash: format!("0x{}", encode(coin.puzzle_hash)),
            amount: coin.amount,
        }
    }
}

impl From<&CoinSpend> for SerializableCoinSpend {
    fn from(coin_spend: &CoinSpend) -> Self {
        SerializableCoinSpend {
            coin: SerializableCoin::from(&coin_spend.coin),
            puzzle_reveal: format!(
                "0x{}",
                encode(coin_spend.puzzle_reveal.clone().into_bytes())
            ),
            solution: format!("0x{}", encode(coin_spend.solution.clone().into_bytes())),
        }
    }
}

impl From<&SpendBundle> for SerializableSpendBundle {
    fn from(spend_bundle: &SpendBundle) -> Self {
        SerializableSpendBundle {
            coin_spends: spend_bundle
                .coin_spends
                .iter()
                .map(SerializableCoinSpend::from)
                .collect(),
            aggregated_signature: format!(
                "0x{}",
                encode(spend_bundle.aggregated_signature.to_bytes())
            ),
        }
    }
}

pub fn get_spend_bundle_json(spends: Vec<CoinSpend>, agg_sig: G2Element) -> String {
    let spend_bundle = SpendBundle {
        coin_spends: spends,
        aggregated_signature: agg_sig,
    };

    let serializable_bundle = SerializableSpendBundle::from(&spend_bundle);
    serde_json::to_string(&serializable_bundle).expect("Serialization failed")
}

pub fn print_spend_bundle(spends: Vec<CoinSpend>, agg_sig: G2Element) {
    println!("{}", get_spend_bundle_json(spends, agg_sig));
}

pub fn print_spend_bundle_to_file(spends: Vec<CoinSpend>, agg_sig: G2Element, file_path: &str) {
    let json_string = get_spend_bundle_json(spends, agg_sig);
    let path = Path::new(file_path);
    let mut file = File::create(path).expect("Unable to create file");
    file.write_all(json_string.as_bytes())
        .expect("Unable to write data");
}
