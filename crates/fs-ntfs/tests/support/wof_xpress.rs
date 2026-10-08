pub fn xpress(size: u16) -> Vec<u8> {
    let mut data = vec![0; 256];
    data[32] = 0x10;
    data[135] = 0x10;
    data.extend_from_slice(&[0, 0x40, 0, 0, 255]);
    data.extend_from_slice(&(size - 4).to_le_bytes());
    data
}
