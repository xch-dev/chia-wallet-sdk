from chia_wallet_sdk import Clvm, PublicKey, RunCatTail, to_hex


def test_alloc():
    clvm = Clvm()

    program = clvm.alloc(
        [
            clvm.nil(),
            PublicKey.infinity(),
            "Hello, world!",
            42,
            100,
            True,
            bytes([1, 2, 3]),
            bytes.fromhex("00" * 32),
            None,
            None,
            RunCatTail(clvm.nil(), clvm.nil()),
        ]
    )

    assert (
        to_hex(program.serialize())
        == "ff80ffb0c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff818fff80ff808080"
    )
