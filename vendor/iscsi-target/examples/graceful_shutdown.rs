//! Example demonstrating graceful shutdown of an iSCSI target
//!
//! This shows how to:
//! 1. Start an iSCSI target
//! 2. Handle a shutdown signal (e.g., Ctrl+C)
//! 3. Reject new logins with SERVICE_UNAVAILABLE
//! 4. Allow existing sessions to complete
//! 5. Stop the target cleanly

use iscsi_target::{IscsiTarget, ScsiBlockDevice, ScsiResult};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Simple in-memory storage
struct MemoryStorage {
    data: Vec<u8>,
}

impl MemoryStorage {
    fn new(size_mb: usize) -> Self {
        MemoryStorage {
            data: vec![0u8; size_mb * 1024 * 1024],
        }
    }
}

impl ScsiBlockDevice for MemoryStorage {
    fn read(&self, lba: u64, blocks: u32, block_size: u32) -> ScsiResult<Vec<u8>> {
        let offset = (lba * block_size as u64) as usize;
        let len = (blocks * block_size) as usize;
        if offset + len > self.data.len() {
            return Err(iscsi_target::IscsiError::Scsi("Read out of bounds".into()));
        }
        Ok(self.data[offset..offset + len].to_vec())
    }

    fn write(&mut self, lba: u64, data: &[u8], block_size: u32) -> ScsiResult<()> {
        let offset = (lba * block_size as u64) as usize;
        if offset + data.len() > self.data.len() {
            return Err(iscsi_target::IscsiError::Scsi("Write out of bounds".into()));
        }
        self.data[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn capacity(&self) -> u64 {
        (self.data.len() / 512) as u64
    }

    fn block_size(&self) -> u32 {
        512
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let bind_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:3261".to_string());

    println!("===========================================");
    println!("Graceful Shutdown Example");
    println!("===========================================");
    println!();
    println!("This example demonstrates graceful shutdown:");
    println!("1. Target starts and accepts connections");
    println!("2. Send SIGINT (Ctrl+C) to initiate graceful shutdown");
    println!("3. New logins are rejected with SERVICE_UNAVAILABLE");
    println!("4. Existing sessions can continue working");
    println!("5. Send SIGINT again to force immediate shutdown");
    println!();

    // Create storage and target
    let storage = MemoryStorage::new(100); // 100 MB
    let target = IscsiTarget::builder()
        .bind_addr(&bind_addr)
        .target_name("iqn.2025-12.local:storage.graceful-shutdown-demo")
        .build(storage)?;

    println!("iSCSI target configured:");
    println!("  Target name: iqn.2025-12.local:storage.graceful-shutdown-demo");
    println!("  Listen address: {}", bind_addr);
    println!();
    println!("To test graceful shutdown:");
    println!("1. Connect a client:");
    println!("   cargo run --example client_connect -- {}", bind_addr);
    println!();
    println!("2. Press Ctrl+C to start graceful shutdown");
    println!("   - Existing sessions continue to work");
    println!("   - New login attempts are rejected");
    println!();
    println!("3. Press Ctrl+C again to force shutdown");
    println!();
    println!("Starting target...");
    println!();

    // Demonstrate programmatic shutdown (in production, you'd use signal handlers)
    // Wrap target in Arc so we can share it between threads
    let target = Arc::new(target);
    let target_clone = Arc::clone(&target);

    // Spawn a thread to run the target
    let target_thread = thread::spawn(move || {
        target_clone.run()
    });

    // Give target time to start
    thread::sleep(Duration::from_secs(1));
    println!("Target is running and accepting connections.");
    println!();

    // Simulate shutdown after 5 seconds
    println!("Waiting 5 seconds before initiating graceful shutdown...");
    thread::sleep(Duration::from_secs(5));

    println!("\n===========================================");
    println!("Initiating graceful shutdown...");
    println!("===========================================");
    println!("- New logins will be rejected with SERVICE_UNAVAILABLE (0x0301)");
    println!("- Existing sessions can continue to operate");
    println!();

    target.shutdown_gracefully();

    // Wait a bit for any sessions to complete
    println!("Waiting 10 seconds for sessions to complete...");
    thread::sleep(Duration::from_secs(10));

    // Stop the target
    println!("\n===========================================");
    println!("Stopping target...");
    println!("===========================================");
    target.stop();

    // Wait for target thread to finish
    let _ = target_thread.join();

    println!("\n===========================================");
    println!("Target shut down cleanly");
    println!("===========================================");

    Ok(())
}
