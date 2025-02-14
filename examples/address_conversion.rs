use chia_sdk_utils::{decode_address, encode_address, AddressInfo};
use hex_literal::hex;

fn main() -> anyhow::Result<()> {
    let puzzle_hash =
        hex!("aca490e9f3ebcafa3d5342d347db2703b31029511f5b40c11441af1c961f6585").into();

    let address = encode_address(puzzle_hash, "xch")?;

    println!("XCH address: {address}");

    let roundtrip = decode_address(&address)?;
    println!(
        "Address matches puzzle hash: {}",
        roundtrip
            == AddressInfo {
                puzzle_hash,
                prefix: "xch".to_string()
            }
    );

    Ok(())
}
