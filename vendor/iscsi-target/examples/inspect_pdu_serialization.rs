use iscsi_target::pdu::IscsiPdu;
use iscsi_target::scsi::SenseData;

fn main() {
    // Create sense data for invalid command
    let sense = SenseData::invalid_command();
    let sense_bytes = sense.to_bytes();

    println!("Sense data structure:");
    println!("  sense_key: 0x{:02x}", sense.sense_key);
    println!("  asc: 0x{:02x}", sense.asc);
    println!("  ascq: 0x{:02x}", sense.ascq);
    println!();

    println!("Sense data bytes (18 bytes total):");
    for (i, byte) in sense_bytes.iter().enumerate() {
        println!("  [{}] = 0x{:02x}", i, byte);
    }
    println!();

    // Create SCSI Response PDU with CHECK CONDITION and sense data
    let pdu = IscsiPdu::scsi_response(
        0x12345678,  // itt
        1,           // stat_sn
        1,           // exp_cmd_sn
        1,           // max_cmd_sn
        0x02,        // status: CHECK CONDITION
        0,           // response code: completed
        0,           // residual count
        Some(&sense_bytes),
    );

    println!("SCSI Response PDU:");
    println!("  opcode: 0x{:02x}", pdu.opcode);
    println!("  flags: 0x{:02x}", pdu.flags);
    println!("  itt: 0x{:08x}", pdu.itt);
    println!("  data_length: {}", pdu.data_length);
    println!("  data.len(): {}", pdu.data.len());
    println!("  specific[0] (response): 0x{:02x}", pdu.specific[0]);
    println!("  specific[1] (status): 0x{:02x}", pdu.specific[1]);
    println!();

    let pdu_bytes = pdu.to_bytes();
    println!("PDU serialized ({} bytes total):", pdu_bytes.len());
    println!();

    println!("BHS (first 48 bytes):");
    for (i, byte) in pdu_bytes[0..48].iter().enumerate() {
        if i % 16 == 0 && i != 0 {
            println!();
        }
        print!("  [{:2}]=0x{:02x}", i, byte);
    }
    println!();
    println!();

    println!("Data segment ({} bytes):", pdu_bytes.len() - 48);
    for (i, byte) in pdu_bytes[48..].iter().enumerate() {
        if i % 16 == 0 && i != 0 {
            println!();
        }
        print!("  [{:2}]=0x{:02x}", i, byte);
    }
    println!();
}
