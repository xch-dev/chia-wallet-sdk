use chia_wallet_sdk::{decode_address, decode_puzzle_hash, encode_address, encode_puzzle_hash};
use hex_literal::hex;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let puzzle_hash = hex!("aca490e9f3ebcafa3d5342d347db2703b31029511f5b40c11441af1c961f6585");
    let encoded_puzzle_hash = encode_puzzle_hash(puzzle_hash, true);

    let address = encode_address(puzzle_hash, "xch")?;

    println!("Puzzle hash: {}", encoded_puzzle_hash);
    println!("XCH address: {}", address);

    let roundtrip = decode_address(&address)?;
    println!(
        "Address matches puzzle hash: {}",
        roundtrip == (puzzle_hash, "xch".to_string())
    );

    let roundtrip = decode_puzzle_hash(&encoded_puzzle_hash)?;
    println!("Puzzle hash matches: {}", roundtrip == puzzle_hash);

    Ok(())
}
