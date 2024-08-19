use clvm_utils::TreeHash;
use hex_literal::hex;

pub const WRITER_FILTER_PUZZLE: [u8; 110] = hex!(
    "
    ff02ffff01ff02ff02ffff04ff02ffff04ffff02ff05ff0b80ff80808080ffff04ffff01ff02ffff
    03ff05ffff01ff02ffff03ffff09ff11ffff0181f380ffff01ff0880ffff01ff04ff09ffff02ff02
    ffff04ff02ffff04ff0dff808080808080ff0180ff8080ff0180ff018080
    "
);

pub const WRITER_FILTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    407f70ea751c25052708219ae148b45db2f61af2287da53d600b2486f12b3ca6
    "
));
