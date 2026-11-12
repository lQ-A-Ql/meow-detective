pub fn hive_bytes() -> Vec<u8> {
    let mut b = vec![0; 8192];
    b[..4].copy_from_slice(b"regf");
    for (at, v) in [
        (4, 1u32),
        (8, 1),
        (0x14, 1),
        (0x18, 5),
        (0x20, 1),
        (0x24, 32),
        (0x28, 4096),
    ] {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    b[4096..4100].copy_from_slice(b"hbin");
    b[4104..4108].copy_from_slice(&4096u32.to_le_bytes());
    b[4128..4132].copy_from_slice(&(-88i32).to_le_bytes());
    b[4132..4134].copy_from_slice(b"nk");
    b[4134..4136].copy_from_slice(&0x20u16.to_le_bytes());
    b[4168..4172].copy_from_slice(&1u32.to_le_bytes());
    b[4172..4176].copy_from_slice(&120u32.to_le_bytes());
    b[4204..4206].copy_from_slice(&4u16.to_le_bytes());
    b[4208..4212].copy_from_slice(b"ROOT");
    b[4216..4220].copy_from_slice(&(-8i32).to_le_bytes());
    b[4220..4224].copy_from_slice(&128u32.to_le_bytes());
    b[4224..4228].copy_from_slice(&(-24i32).to_le_bytes());
    b[4228..4230].copy_from_slice(b"vk");
    b[4232..4236].copy_from_slice(&0x80000004u32.to_le_bytes());
    b[4236..4240].copy_from_slice(&42u32.to_le_bytes());
    b[4240..4244].copy_from_slice(&4u32.to_le_bytes());
    b[4248..4252].copy_from_slice(&3944i32.to_le_bytes());
    b
}
