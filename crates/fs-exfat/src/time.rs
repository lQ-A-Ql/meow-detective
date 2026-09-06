use chrono::{DateTime, NaiveDate, Utc};

pub(crate) fn parse_exfat_date_time(
    date: u16,
    time: u16,
    increment_10ms: u8,
    utc_offset: u8,
) -> Option<DateTime<Utc>> {
    if date == 0 && time == 0 {
        return None;
    }
    let year = ((date >> 9) & 0x7F) as i32 + 1980;
    let month = (date >> 5) & 0x0F;
    let day = date & 0x1F;
    let hour = (time >> 11) & 0x1F;
    let minute = (time >> 5) & 0x3F;
    let second = (time & 0x1F) * 2;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let naive = NaiveDate::from_ymd_opt(year, u32::from(month), u32::from(day))?
        .and_hms_milli_opt(
            u32::from(hour),
            u32::from(minute),
            u32::from(second),
            u32::from(increment_10ms) * 10,
        )?;
    Some((naive - chrono::Duration::minutes(exfat_offset_minutes(utc_offset))).and_utc())
}

fn exfat_offset_minutes(utc_offset: u8) -> i64 {
    if utc_offset == 0xFF {
        0
    } else if utc_offset <= 0xDF {
        i64::from(utc_offset) * 15
    } else {
        (i64::from(utc_offset) - 256) * 15
    }
}
