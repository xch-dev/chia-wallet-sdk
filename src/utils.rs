pub fn u64_to_bytes(amount: u64) -> Vec<u8> {
    let bytes: Vec<u8> = amount.to_be_bytes().into();
    let mut slice = bytes.as_slice();

    // Remove leading zeros.
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }

    slice.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes() {
        assert_eq!(u64_to_bytes(0), Vec::<u8>::new());
        assert_eq!(u64_to_bytes(1), &[1]);
        assert_eq!(u64_to_bytes(45213), &[0, 176, 157]);
        assert_eq!(
            u64_to_bytes(u64::MAX),
            &[255, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(u64_to_bytes(1721349832147), &[1, 144, 200, 113, 253, 211]);
        assert_eq!(u64_to_bytes(10000), &[39, 16]);
        assert_eq!(u64_to_bytes(1000), &[3, 232]);
        assert_eq!(
            u64_to_bytes(u64::MAX - 1),
            &[255, 255, 255, 255, 255, 255, 255, 254]
        );
        assert_eq!(
            u64_to_bytes(u64::MAX / 2),
            &[127, 255, 255, 255, 255, 255, 255, 255]
        );
    }
}
