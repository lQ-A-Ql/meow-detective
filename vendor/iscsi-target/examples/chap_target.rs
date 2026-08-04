//! iSCSI target example with CHAP authentication
//!
//! This example demonstrates how to create an iSCSI target with
//! CHAP authentication enabled for Microsoft Windows certification.

use iscsi_target::{AuthConfig, ChapCredentials, IscsiError, IscsiTarget, ScsiBlockDevice, ScsiResult};

/// Simple in-memory storage backend
struct MemoryStorage {
    data: Vec<u8>,
    block_size: u32,
}

impl MemoryStorage {
    fn new(size_mb: usize, block_size: u32) -> Self {
        let size_bytes = size_mb * 1024 * 1024;
        Self {
            data: vec![0u8; size_bytes],
            block_size,
        }
    }
}

impl ScsiBlockDevice for MemoryStorage {
    fn read(&self, lba: u64, blocks: u32, block_size: u32) -> ScsiResult<Vec<u8>> {
        if block_size != self.block_size {
            return Err(IscsiError::Scsi(format!(
                "block size mismatch: expected {}, got {}",
                self.block_size, block_size
            )));
        }

        let offset = (lba * block_size as u64) as usize;
        let len = (blocks * block_size) as usize;

        if offset + len > self.data.len() {
            return Err(IscsiError::Scsi(format!(
                "read beyond device capacity: LBA {}, blocks {}",
                lba, blocks
            )));
        }

        Ok(self.data[offset..offset + len].to_vec())
    }

    fn write(&mut self, lba: u64, data: &[u8], block_size: u32) -> ScsiResult<()> {
        if block_size != self.block_size {
            return Err(IscsiError::Scsi(format!(
                "block size mismatch: expected {}, got {}",
                self.block_size, block_size
            )));
        }

        let offset = (lba * block_size as u64) as usize;

        if offset + data.len() > self.data.len() {
            return Err(IscsiError::Scsi(format!(
                "write beyond device capacity: LBA {}, bytes {}",
                lba,
                data.len()
            )));
        }

        self.data[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn capacity(&self) -> u64 {
        (self.data.len() / self.block_size as usize) as u64
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn flush(&mut self) -> ScsiResult<()> {
        // No-op for memory storage
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create 100 MB in-memory storage with 512-byte blocks
    let storage = MemoryStorage::new(100, 512);

    println!("Creating iSCSI target with CHAP authentication");
    println!("Storage: {} MB in-memory", 100);
    println!(
        "Capacity: {} blocks of {} bytes",
        storage.capacity(),
        storage.block_size()
    );

    // Configure CHAP authentication
    let auth = AuthConfig::Chap {
        credentials: ChapCredentials::new("iscsi-user", "secretpassword123"),
    };

    println!("\nCHAP Authentication:");
    println!("  Username: iscsi-user");
    println!("  Password: secretpassword123");

    // Build and configure the target
    let target = IscsiTarget::builder()
        .bind_addr("0.0.0.0:3262")
        .target_name("iqn.2025-12.local:storage.chap-disk")
        .with_auth(auth)
        .build(storage)?;

    println!("\niSCSI target configured:");
    println!("  Target name: iqn.2025-12.local:storage.chap-disk");
    println!("  Listen address: 0.0.0.0:3262");
    println!("  Authentication: CHAP (one-way)");

    println!("\nTo connect from Linux:");
    println!("  1. Configure CHAP credentials in /etc/iscsi/iscsid.conf:");
    println!("     node.session.auth.authmethod = CHAP");
    println!("     node.session.auth.username = iscsi-user");
    println!("     node.session.auth.password = secretpassword123");
    println!("\n  2. Discover and login:");
    println!("     sudo iscsiadm -m discovery -t sendtargets -p 127.0.0.1:3262");
    println!("     sudo iscsiadm -m node -T iqn.2025-12.local:storage.chap-disk -p 127.0.0.1:3262 --login");

    println!("\nTo connect from Windows:");
    println!("  1. Open iSCSI Initiator");
    println!("  2. Discovery tab -> Add target portal: 127.0.0.1");
    println!("  3. Targets tab -> Select target -> Connect");
    println!("  4. Advanced Settings -> Enable CHAP login");
    println!("  5. Enter username and password");

    println!("\nStarting iSCSI target server with CHAP authentication...\n");

    // Run the target
    match target.run() {
        Ok(_) => {
            println!("Target stopped gracefully");
            Ok(())
        }
        Err(e) => {
            eprintln!("Target error: {}", e);
            Err(e.into())
        }
    }
}
