use chia_consensus::consensus_constants::ConsensusConstants;
use chia_protocol::Bytes32;
use clvmr::sha2::Sha256;
use hex_literal::hex;
use once_cell::sync::Lazy;

const MAINNET_GENESIS_CHALLENGE: Bytes32 = Bytes32::new(hex!(
    "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb"
));

pub static MAINNET_CONSTANTS: Lazy<ConsensusConstants> = Lazy::new(|| ConsensusConstants {
    slot_blocks_target: 32,
    min_blocks_per_challenge_block: 16,
    max_sub_slot_blocks: 128,
    num_sps_sub_slot: 64,
    sub_slot_iters_starting: 2u64.pow(27),
    difficulty_constant_factor: 2u128.pow(67),
    difficulty_starting: 7,
    difficulty_change_max_factor: 3,
    sub_epoch_blocks: 384,
    epoch_blocks: 4608,
    significant_bits: 8,
    discriminant_size_bits: 1024,
    number_zero_bits_plot_filter: 9,
    min_plot_size: 32,
    max_plot_size: 50,
    sub_slot_time_target: 600,
    num_sp_intervals_extra: 3,
    max_future_time2: 120,
    number_of_timestamps: 11,
    genesis_challenge: MAINNET_GENESIS_CHALLENGE,
    agg_sig_me_additional_data: MAINNET_GENESIS_CHALLENGE,
    agg_sig_parent_additional_data: hash(MAINNET_GENESIS_CHALLENGE, 43),
    agg_sig_puzzle_additional_data: hash(MAINNET_GENESIS_CHALLENGE, 44),
    agg_sig_amount_additional_data: hash(MAINNET_GENESIS_CHALLENGE, 45),
    agg_sig_puzzle_amount_additional_data: hash(MAINNET_GENESIS_CHALLENGE, 46),
    agg_sig_parent_amount_additional_data: hash(MAINNET_GENESIS_CHALLENGE, 47),
    agg_sig_parent_puzzle_additional_data: hash(MAINNET_GENESIS_CHALLENGE, 48),
    genesis_pre_farm_pool_puzzle_hash: Bytes32::new(hex!(
        "d23da14695a188ae5708dd152263c4db883eb27edeb936178d4d988b8f3ce5fc"
    )),
    genesis_pre_farm_farmer_puzzle_hash: Bytes32::new(hex!(
        "3d8765d3a597ec1d99663f6c9816d915b9f68613ac94009884c4addaefcce6af"
    )),
    max_vdf_witness_size: 64,
    mempool_block_buffer: 10,
    max_coin_amount: u64::MAX,
    max_block_cost_clvm: 11_000_000_000,
    cost_per_byte: 12_000,
    weight_proof_threshold: 2,
    blocks_cache_size: 4608 + 128 * 4,
    weight_proof_recent_blocks: 1000,
    max_block_count_per_requests: 32,
    max_generator_size: 1_000_000,
    max_generator_ref_list_size: 512,
    pool_sub_slot_iters: 37_600_000_000,
    soft_fork2_height: 0,
    soft_fork4_height: 5_716_000,
    soft_fork5_height: 5_940_000,
    hard_fork_height: 5_496_000,
    hard_fork_fix_height: 0,
    plot_filter_128_height: 10_542_000,
    plot_filter_64_height: 15_592_000,
    plot_filter_32_height: 20_643_000,
});

fn hash(agg_sig_data: Bytes32, byte: u8) -> Bytes32 {
    let mut hasher = Sha256::new();
    hasher.update(agg_sig_data);
    hasher.update([byte]);
    hasher.finalize().into()
}
