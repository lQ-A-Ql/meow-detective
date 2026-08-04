use iscsi_target::client::IscsiClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let portal = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:3260".to_string());

    let initiator_iqn = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "iqn.2025-12.local:test-initiator".to_string());

    println!("Discovering targets at {}...", portal);

    let mut client = IscsiClient::connect(&portal)?;
    let targets = client.discover(&initiator_iqn)?;

    if targets.is_empty() {
        println!("No targets discovered");
    } else {
        println!("\nDiscovered {} target(s):", targets.len());
        for (iqn, addr) in &targets {
            println!("  TargetName: {}", iqn);
            println!("  TargetAddress: {}", addr);
            println!();
        }
    }

    Ok(())
}
