/// Fast deterministic hash matching the TypeScript implementation over UTF-16
/// code units.
pub fn short_hash(text: &str) -> String {
    let mut h1: i32 = 0xdeadbeefu32 as i32;
    let mut h2: i32 = 0x41c6ce57u32 as i32;

    for unit in text.encode_utf16() {
        h1 = (h1 ^ i32::from(unit)).wrapping_mul(2_654_435_761u32 as i32);
        h2 = (h2 ^ i32::from(unit)).wrapping_mul(1_597_334_677u32 as i32);
    }

    h1 = (h1 ^ unsigned_shift(h1, 16)).wrapping_mul(2_246_822_507u32 as i32)
        ^ (h2 ^ unsigned_shift(h2, 13)).wrapping_mul(3_266_489_909u32 as i32);
    h2 = (h2 ^ unsigned_shift(h2, 16)).wrapping_mul(2_246_822_507u32 as i32)
        ^ (h1 ^ unsigned_shift(h1, 13)).wrapping_mul(3_266_489_909u32 as i32);

    format!("{}{}", to_base36(h2 as u32), to_base36(h1 as u32))
}

fn unsigned_shift(value: i32, shift: u32) -> i32 {
    ((value as u32) >> shift) as i32
}

fn to_base36(mut value: u32) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_owned();
    }

    let mut reversed = Vec::new();
    while value > 0 {
        reversed.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    reversed.reverse();
    String::from_utf8(reversed).expect("base36 digits are ASCII")
}
