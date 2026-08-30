#[test]
fn hostile_frame_lengths_are_rejected_before_allocation() {
    for (input, output) in [(0u32, 1u32), (4097, 1), (1, 65537), (u32::MAX, u32::MAX)] {
        let mut header = [0; 16];
        header[..4].copy_from_slice(&input.to_be_bytes());
        header[4..8].copy_from_slice(&output.to_be_bytes());
        assert!(super::lengths(&header).is_err());
    }
}

#[test]
fn response_preserves_binary_information_and_sequence_identity() {
    assert_eq!(super::response(&[0, 255], 4, 255), [255, 255, 1, 253]);
    assert_ne!(
        super::response(&[1, 2], 32, 0),
        super::response(&[1, 2], 32, 1)
    );
}
