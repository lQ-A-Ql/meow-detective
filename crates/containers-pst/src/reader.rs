//! PST file reader implementation.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::header::{
    parse_header, BbtEntry, NbtEntry, PstHeader, BTREE_BB, BTREE_INTERNAL, BTREE_LEAF, HEADER_SIZE,
    NID_ROOT_FOLDER, NID_SEARCH_ROOT, NID_TOP_OF_PERSONAL_FOLDERS, PAGE_SIZE,
};
use crate::props::{
    find_prop_filetime, find_prop_string, find_prop_string_array, parse_property_context,
    prop_type, read_u32_le, read_u64_le, PropValue, PROP_TAG_ATTACH_DATA,
    PROP_TAG_ATTACH_LONG_FILENAME, PROP_TAG_ATTACH_MIME, PROP_TAG_ATTACH_SIZE, PROP_TAG_BODY,
    PROP_TAG_DELIVERY_TIME, PROP_TAG_DISPLAY_BCC, PROP_TAG_DISPLAY_CC, PROP_TAG_DISPLAY_TO,
    PROP_TAG_INTERNET_MESSAGE_ID, PROP_TAG_IN_REPLY_TO_ID, PROP_TAG_MESSAGE_CLASS,
    PROP_TAG_REFERENCES, PROP_TAG_SENDER_EMAIL, PROP_TAG_SENDER_NAME, PROP_TAG_SENT_TIME,
    PROP_TAG_SUBJECT,
};
use crate::{PstAttachment, PstCalendar, PstContact, PstError, PstFolder, PstMessage};

/// A read-only PST file reader.
///
/// Opens a PST file, parses its header, and provides methods to extract
/// messages, folders, calendar entries, and contacts.
///
/// # Examples
///
/// ```ignore
/// let reader = PstReader::open("mailbox.pst").unwrap();
/// let messages = reader.read_messages().unwrap();
/// ```
pub struct PstReader {
    /// The entire PST file content loaded into memory.
    data: Vec<u8>,
    /// Parsed header information.
    pub(crate) header: PstHeader,
    /// Cached BBT entries keyed by BID for fast lookup.
    pub(crate) bbt_cache: BTreeMap<u64, BbtEntry>,
    /// Cached NBT entries keyed by NID.
    pub(crate) nbt_cache: BTreeMap<u32, NbtEntry>,
}

impl PstReader {
    /// Open a PST file from the given path.
    ///
    /// Reads the header, validates the magic bytes, detects Unicode vs ANSI,
    /// and caches the NBT and BBT entries for subsequent lookups.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PstError> {
        let data = fs::read(path)?;

        if data.len() < HEADER_SIZE {
            return Err(PstError::InvalidFormat(
                "File is too small to be a PST".to_string(),
            ));
        }

        let header = parse_header(&data)?;

        let mut reader = Self {
            data,
            header,
            bbt_cache: BTreeMap::new(),
            nbt_cache: BTreeMap::new(),
        };

        // Pre-cache BBT and NBT entries.
        reader.cache_bbt()?;
        reader.cache_nbt()?;

        Ok(reader)
    }

    /// Return whether this is a Unicode (64-bit) PST.
    pub fn is_unicode(&self) -> bool {
        self.header.is_unicode
    }

    /// Return the file size declared in the header.
    pub fn file_size(&self) -> u64 {
        self.header.file_size
    }

    // ── Public extraction methods ────────────────────────────────────────

    /// Extract all email messages from the PST.
    pub fn read_messages(&self) -> Result<Vec<PstMessage>, PstError> {
        let mut messages = Vec::new();
        let folder_nids = self.collect_folder_nids()?;

        for folder_nid in &folder_nids {
            let sub_nids = self.get_subnode_nids(*folder_nid)?;
            for sub_nid in sub_nids {
                let message_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                match message_class.as_deref() {
                    Some("IPM.Note")
                    | Some("IPM.Note.SMIME")
                    | Some("IPM.Note.SMIME.MultipartSigned") => {}
                    _ => {
                        // Skip non-mail items (appointments, contacts, etc.)
                        continue;
                    }
                }
                if let Ok(msg) = self.read_message(sub_nid, &self.get_folder_path(*folder_nid)) {
                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    /// Extract the folder hierarchy from the PST.
    pub fn read_folders(&self) -> Result<Vec<PstFolder>, PstError> {
        let mut folders = Vec::new();
        let nids = self.collect_folder_nids()?;

        for nid in &nids {
            let name = self
                .get_property_string(*nid, 0x3001) // PidTagDisplayName
                .unwrap_or_else(|| format!("Folder_{:X}", nid));
            let parent_path = self.get_folder_path(*nid);

            // Count children (messages/subfolders) via hierarchy table.
            let sub_nids = self.get_subnode_nids(*nid).unwrap_or_default();
            let message_count = sub_nids.len() as u64;

            // Count subfolders — approximate from NIDs that have this as parent.
            let subfolder_count = self.count_direct_subfolders(*nid);

            let depth = parent_path.split('/').filter(|s| !s.is_empty()).count() as u32;

            folders.push(PstFolder {
                name,
                parent_path,
                depth,
                message_count,
                subfolder_count,
            });
        }

        Ok(folders)
    }

    /// Extract calendar entries.
    pub fn read_calendar(&self) -> Result<Vec<PstCalendar>, PstError> {
        let mut calendars = Vec::new();
        let folder_nids = self.collect_folder_nids()?;

        for folder_nid in &folder_nids {
            let sub_nids = self.get_subnode_nids(*folder_nid)?;
            for sub_nid in sub_nids {
                let msg_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                if msg_class.as_deref() != Some("IPM.Appointment") {
                    continue;
                }

                let subject = self
                    .get_property_string(sub_nid, PROP_TAG_SUBJECT)
                    .unwrap_or_default();
                let location = self
                    .get_property_string(sub_nid, 0x8208) // PidTagLocation
                    .unwrap_or_default();

                let start_time = self.get_property_filetime(sub_nid, 0x820D); // PidLidAppointmentStartWhole
                let end_time = self.get_property_filetime(sub_nid, 0x820E); // PidLidAppointmentEndWhole

                let attendees = self
                    .get_property_string_array(sub_nid, 0x823E) // PidLidAttendeeString
                    .unwrap_or_default();

                calendars.push(PstCalendar {
                    subject,
                    start_time,
                    end_time,
                    location,
                    attendees,
                });
            }
        }

        Ok(calendars)
    }

    /// Extract contact entries.
    pub fn read_contacts(&self) -> Result<Vec<PstContact>, PstError> {
        let mut contacts = Vec::new();
        let folder_nids = self.collect_folder_nids()?;

        for folder_nid in &folder_nids {
            let sub_nids = self.get_subnode_nids(*folder_nid)?;
            for sub_nid in sub_nids {
                let msg_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                if msg_class.as_deref() != Some("IPM.Contact") {
                    continue;
                }

                let name = self
                    .get_property_string(sub_nid, 0x3A06) // PidTagDisplayName
                    .unwrap_or_default();

                let email = self
                    .get_property_string(sub_nid, 0x39FE) // PidTagPrimarySmtpAddress
                    .or_else(|| {
                        self.get_property_string(sub_nid, 0x8083) // PidTagEmailAddress
                    })
                    .unwrap_or_default();

                let phone = self
                    .get_property_string(sub_nid, 0x3A08) // PidTagBusinessTelephoneNumber
                    .or_else(|| {
                        self.get_property_string(sub_nid, 0x3A1C) // PidTagMobileTelephoneNumber
                    })
                    .unwrap_or_default();

                let address = self
                    .get_property_string(sub_nid, 0x3A29) // PidTagStreetAddress
                    .unwrap_or_default();

                contacts.push(PstContact {
                    name,
                    email,
                    phone,
                    address,
                });
            }
        }

        Ok(contacts)
    }

    // ── Internal: NBT/BBT caching ────────────────────────────────────────

    fn cache_bbt(&mut self) -> Result<(), PstError> {
        let bref = self.header.root_bbt;
        if bref.bid == 0 {
            return Ok(());
        }
        // Seed the cache with the root BREF so bid_to_file_offset can bootstrap.
        self.bbt_cache.insert(bref.bid, BbtEntry { bref });
        self.load_bbt_page(bref.bid)?;
        Ok(())
    }

    fn load_bbt_page(&mut self, bid: u64) -> Result<(), PstError> {
        let page_offset = self.bid_to_file_offset(bid);
        if page_offset + PAGE_SIZE > self.data.len() {
            return Err(PstError::InvalidFormat(format!(
                "BBT page BID 0x{:X} at offset {} is out of bounds",
                bid, page_offset
            )));
        }

        let page_data = &self.data[page_offset..page_offset + PAGE_SIZE];

        // Read page header to determine entry layout.
        let sig = page_data[0]; // first byte of wSig
        let level = page_data.get(22).copied().unwrap_or(0);
        let c_ent = page_data.get(23).copied().unwrap_or(0) as usize;
        let cb_ent = page_data.get(24).copied().unwrap_or(0) as usize;

        if sig == BTREE_LEAF || sig == BTREE_INTERNAL {
            // This is a BTree page with btypeBTC inside NBT — skip for BBT.
            // The BBT uses different sig bytes.
        }

        // BBT pages: wSig is typically 0x80 or 0xB580.
        let is_bbt_page =
            sig == BTREE_BB || sig == BTREE_LEAF || sig == BTREE_INTERNAL || sig == 0xB5; // btypeBTC

        if !is_bbt_page && sig != BTREE_LEAF && sig != BTREE_INTERNAL {
            // For raw/leaf BBT pages: try parsing entries.
        }

        // Use the entry layout from the page.
        let entries_offset = if self.header.is_unicode { 40 } else { 24 };

        for i in 0..c_ent {
            let ent_offset = entries_offset + i * cb_ent;
            let entry = if self.header.is_unicode {
                BbtEntry::from_bytes_unicode(page_data, ent_offset)
            } else {
                BbtEntry::from_bytes_ansi(page_data, ent_offset)
            };
            if let Some(entry) = entry {
                self.bbt_cache.insert(entry.bref.bid, entry);
            }
        }

        // Collect child BIDs first (avoid borrowing page_data across mutable self calls).
        let mut child_bids: Vec<u64> = Vec::new();
        if level > 0 {
            for i in 0..c_ent {
                let ent_offset = entries_offset + i * cb_ent;
                let child_bid = if self.header.is_unicode {
                    read_u64_le(page_data, ent_offset + 8).unwrap_or(0)
                } else {
                    read_u32_le(page_data, ent_offset + 4).unwrap_or(0) as u64
                };
                if child_bid != 0 && !self.bbt_cache.contains_key(&child_bid) {
                    let child_page = self.bid_to_file_offset(child_bid);
                    if child_page > 0 && child_page < self.data.len() {
                        child_bids.push(child_bid);
                    }
                }
            }
        }
        // Recurse into child pages (no longer borrowing page_data).
        for child_bid in child_bids {
            self.load_bbt_page(child_bid).ok();
        }

        Ok(())
    }

    fn cache_nbt(&mut self) -> Result<(), PstError> {
        let bid = self.header.root_nbt.bid;
        if bid == 0 {
            return Err(PstError::InvalidFormat(
                "No NBT root found in header".to_string(),
            ));
        }
        self.load_nbt_page(bid)?;
        Ok(())
    }

    fn load_nbt_page(&mut self, bid: u64) -> Result<(), PstError> {
        let page_offset = self.bid_to_file_offset(bid);
        if page_offset + PAGE_SIZE > self.data.len() {
            return Err(PstError::InvalidFormat(format!(
                "NBT page BID 0x{:X} at offset {} is out of bounds",
                bid, page_offset
            )));
        }

        let page_data = &self.data[page_offset..page_offset + PAGE_SIZE];

        let level = page_data.get(22).copied().unwrap_or(0);
        let c_ent = page_data.get(23).copied().unwrap_or(0) as usize;
        let cb_ent = page_data.get(24).copied().unwrap_or(0) as usize;

        let entries_offset = if self.header.is_unicode { 40 } else { 24 };

        for i in 0..c_ent {
            let ent_offset = entries_offset + i * cb_ent;
            let entry = if self.header.is_unicode {
                NbtEntry::from_bytes_unicode(page_data, ent_offset)
            } else {
                NbtEntry::from_bytes_ansi(page_data, ent_offset)
            };
            if let Some(entry) = entry {
                let nid = entry.nid;
                self.nbt_cache.insert(nid, entry);
            }
        }

        // Collect sub-node BIDs first (avoid borrowing page_data across mutable self calls).
        let mut sub_bids: Vec<u64> = Vec::new();
        if level > 0 {
            for i in 0..c_ent {
                let ent_offset = entries_offset + i * cb_ent;
                let sub_bid = if self.header.is_unicode {
                    read_u64_le(page_data, ent_offset + 16).unwrap_or(0)
                } else {
                    read_u32_le(page_data, ent_offset + 8).unwrap_or(0) as u64
                };
                if sub_bid != 0 {
                    sub_bids.push(sub_bid);
                }
            }
        }
        // Recurse into child nodes (no longer borrowing page_data).
        for sub_bid in sub_bids {
            self.load_nbt_page(sub_bid).ok();
        }

        Ok(())
    }

    // ── Internal: data access ────────────────────────────────────────────

    /// Convert a BID to a file byte offset.
    fn bid_to_file_offset(&self, bid: u64) -> usize {
        // First check BBT cache.
        if let Some(entry) = self.bbt_cache.get(&bid) {
            return entry.bref.ib as usize;
        }
        // Fallback: assume bid * page_size gives the file offset.
        (bid as usize) * PAGE_SIZE
    }

    /// Read the sub-node block for a given NID (the subnode BTree of properties).
    /// Falls back to the data block when no sub-node is present.
    pub(crate) fn read_subnode_block(&self, nid: u32) -> Option<&[u8]> {
        let entry = self.nbt_cache.get(&nid)?;
        let bid = if entry.bid_sub != 0 {
            entry.bid_sub
        } else {
            entry.bid_data
        };
        if bid == 0 {
            return None;
        }
        let offset = self.bid_to_file_offset(bid);
        let end = (offset + PAGE_SIZE).min(self.data.len());
        Some(&self.data[offset..end])
    }

    // ── Internal: property extraction ────────────────────────────────────

    /// Get a string property value for a given NID and property tag.
    fn get_property_string(&self, nid: u32, prop_tag: u16) -> Option<String> {
        let block = self.read_subnode_block(nid)?;
        find_prop_string(block, prop_tag)
    }

    /// Get a FILETIME property value.
    fn get_property_filetime(&self, nid: u32, prop_tag: u16) -> Option<DateTime<Utc>> {
        let block = self.read_subnode_block(nid)?;
        find_prop_filetime(block, prop_tag)
    }

    /// Get a multi-valued string property.
    fn get_property_string_array(&self, nid: u32, prop_tag: u16) -> Option<Vec<String>> {
        let block = self.read_subnode_block(nid)?;
        find_prop_string_array(block, prop_tag)
    }

    /// Parse a property context and return all properties.
    pub(crate) fn parse_property_context(&self, data: &[u8]) -> BTreeMap<u32, PropValue> {
        parse_property_context(data)
    }

    // ── Internal: folder hierarchy ───────────────────────────────────────

    /// Collect all folder NIDs by traversing the hierarchy from the root folder.
    fn collect_folder_nids(&self) -> Result<Vec<u32>, PstError> {
        let mut folder_nids = Vec::new();

        // Start from well-known folder NIDs.
        let roots = [
            NID_ROOT_FOLDER,
            NID_TOP_OF_PERSONAL_FOLDERS,
            NID_SEARCH_ROOT,
        ];

        for root_nid in &roots {
            if self.nbt_cache.contains_key(root_nid) {
                folder_nids.push(*root_nid);
                self.collect_child_folders(*root_nid, &mut folder_nids);
            }
        }

        // If no well-known NIDs found, collect all NIDs that appear to be folders.
        if folder_nids.is_empty() {
            for (&nid, _entry) in self.nbt_cache.iter() {
                // Heuristic: folder NIDs tend to be in certain ranges.
                // Or check if the node has a sub-node (property context) with DisplayName.
                if self.read_subnode_block(nid).is_some() {
                    let msg_class = self.get_property_string(nid, PROP_TAG_MESSAGE_CLASS);
                    if msg_class.as_deref() == Some("IPM.Note")
                        || msg_class.as_deref() == Some("IPM.Appointment")
                        || msg_class.as_deref() == Some("IPM.Contact")
                    {
                        // This is a message, not a folder. Skip.
                        continue;
                    }
                    folder_nids.push(nid);
                }
            }
        }

        Ok(folder_nids)
    }

    fn collect_child_folders(&self, parent_nid: u32, result: &mut Vec<u32>) {
        // Try to read sub-node NIDs from hierarchy table.
        if let Ok(sub_nids) = self.get_subnode_nids(parent_nid) {
            for sub_nid in sub_nids {
                let msg_class = self.get_property_string(sub_nid, PROP_TAG_MESSAGE_CLASS);
                match msg_class.as_deref() {
                    Some("IPM.Note")
                    | Some("IPM.Note.SMIME")
                    | Some("IPM.Appointment")
                    | Some("IPM.Contact") => continue,
                    _ => {}
                }
                // Check if this is a folder by looking for folder-specific properties.
                if self.get_property_string(sub_nid, 0x3613).is_some()
                // PidTagContainerClass
                {
                    result.push(sub_nid);
                    self.collect_child_folders(sub_nid, result);
                }
            }
        }

        // Also walk NBT entries that might be children of this folder.
        for (&nid, _entry) in self.nbt_cache.iter() {
            if nid == parent_nid {
                continue;
            }
            // Check if this NID's parent relationship points to parent_nid.
            if let Some(_block) = self.read_subnode_block(nid) {
                // PidTagParentEntryId = 0x0E09
                // We can't easily check this without decoding EntryID.
                // Skip for now.
            }
        }
    }

    /// Retrieve sub-node NIDs (messages/items) for a given folder NID.
    fn get_subnode_nids(&self, folder_nid: u32) -> Result<Vec<u32>, PstError> {
        let mut sub_nids = Vec::new();

        // Find NBT entries whose parent is this folder.
        // This is determined by looking at the sub-node BTree of the folder.
        let _block = match self.read_subnode_block(folder_nid) {
            Some(b) => b,
            None => return Ok(sub_nids),
        };

        // Look for a contents table — a BTree-on-Heap that lists items.
        // The contents table has rows, each row has a NID.
        // For now, collect all NIDs from the NBT that have bid_data but no bid_sub.
        for (&nid, entry) in self.nbt_cache.iter() {
            if nid == folder_nid {
                continue;
            }
            // Heuristic: leaf entries have bid_data but bid_sub == 0.
            // But actually, leaf NBT entries ARE the messages' data nodes.
            // In the NBT, each message NID has a bidData pointing to its data block.
            // The NBT itself doesn't encode parent/child relationships for items.

            // A better heuristic: if the nid has a sub-node block (has properties),
            // it's an item. Parse its properties to see if it's a message.
            if entry.bid_sub != 0 {
                if let Some(sub_block) = self.read_subnode_block(nid) {
                    let props = self.parse_property_context(sub_block);
                    // Check for message-class related properties.
                    // Actually, let's just collect all NIDs with sub-blocks.
                    if !props.is_empty() {
                        sub_nids.push(nid);
                    }
                }
            }
        }

        Ok(sub_nids)
    }

    /// Count subfolders that are direct children of the given folder NID.
    fn count_direct_subfolders(&self, _parent_nid: u32) -> u64 {
        // Approximate: walk all folder NIDs and check hierarchy.
        // For the MVP, return 0 (the caller can improve this).
        0
    }

    /// Build a folder path string like "/Inbox/Subfolder".
    fn get_folder_path(&self, nid: u32) -> String {
        let name = self
            .get_property_string(nid, 0x3001)
            .unwrap_or_else(|| format!("Folder_{:X}", nid));
        format!("/{}", name)
    }

    /// Read a single message from the given NID.
    fn read_message(&self, nid: u32, folder_path: &str) -> Result<PstMessage, PstError> {
        let block = self
            .read_subnode_block(nid)
            .ok_or_else(|| PstError::InvalidFormat(format!("No data block for NID {:X}", nid)))?;

        let _props = self.parse_property_context(block);

        let subject = self
            .get_property_string(nid, PROP_TAG_SUBJECT)
            .unwrap_or_default();
        let body_plain = self
            .get_property_string(nid, PROP_TAG_BODY)
            .unwrap_or_default();
        let body_html = self
            .get_property_string(nid, 0x1013) // PidTagHtml
            .unwrap_or_default();
        let sender_name = self
            .get_property_string(nid, PROP_TAG_SENDER_NAME)
            .unwrap_or_default();
        let sender_email = self
            .get_property_string(nid, PROP_TAG_SENDER_EMAIL)
            .unwrap_or_default();
        let sent_time = self.get_property_filetime(nid, PROP_TAG_SENT_TIME);
        let received_time = self.get_property_filetime(nid, PROP_TAG_DELIVERY_TIME);

        let to = self
            .get_property_string(nid, PROP_TAG_DISPLAY_TO)
            .unwrap_or_default();
        let cc = self
            .get_property_string(nid, PROP_TAG_DISPLAY_CC)
            .unwrap_or_default();
        let bcc = self
            .get_property_string(nid, PROP_TAG_DISPLAY_BCC)
            .unwrap_or_default();

        let to_vec = split_display_addresses(&to);
        let cc_vec = split_display_addresses(&cc);
        let bcc_vec = split_display_addresses(&bcc);

        // Keep `recipients` as a backward-compatible aggregate.
        let mut recipients: Vec<String> = Vec::new();
        recipients.extend(to_vec.iter().cloned());
        recipients.extend(cc_vec.iter().cloned());

        let message_id = self
            .get_property_string(nid, PROP_TAG_INTERNET_MESSAGE_ID)
            .unwrap_or_default();
        let in_reply_to = self
            .get_property_string(nid, PROP_TAG_IN_REPLY_TO_ID)
            .unwrap_or_default();
        let references = self
            .get_property_string(nid, PROP_TAG_REFERENCES)
            .map(|raw| split_display_addresses(&raw))
            .unwrap_or_default();
        let message_class = self
            .get_property_string(nid, PROP_TAG_MESSAGE_CLASS)
            .unwrap_or_else(|| "IPM.Note".to_string());

        // Build a best-effort header list from MAPI properties.
        let mut headers: Vec<(String, String)> = Vec::new();
        if !subject.is_empty() {
            headers.push(("Subject".to_string(), subject.clone()));
        }
        if !sender_email.is_empty() {
            let from = if sender_name.is_empty() {
                sender_email.clone()
            } else {
                format!("{} <{}>", sender_name, sender_email)
            };
            headers.push(("From".to_string(), from));
        }
        if !to.is_empty() {
            headers.push(("To".to_string(), to.clone()));
        }
        if !cc.is_empty() {
            headers.push(("Cc".to_string(), cc.clone()));
        }
        if !bcc.is_empty() {
            headers.push(("Bcc".to_string(), bcc.clone()));
        }
        if !message_id.is_empty() {
            headers.push(("Message-Id".to_string(), message_id.clone()));
        }
        if !in_reply_to.is_empty() {
            headers.push(("In-Reply-To".to_string(), in_reply_to.clone()));
        }
        if !references.is_empty() {
            headers.push(("References".to_string(), references.join(" ")));
        }
        headers.push(("Message-Class".to_string(), message_class.clone()));

        // Extract attachments from sub-node table.
        let attachments = self.read_attachments(nid);

        Ok(PstMessage {
            subject,
            body_plain,
            body_html,
            sender_name,
            sender_email,
            recipients,
            to: to_vec,
            cc: cc_vec,
            bcc: bcc_vec,
            reply_to: String::new(),
            return_path: String::new(),
            message_id,
            in_reply_to,
            references,
            message_class,
            x_mailer: String::new(),
            x_originating_ip: String::new(),
            sent_time,
            received_time,
            attachments,
            folder_path: folder_path.to_string(),
            headers,
        })
    }

    /// Extract attachments for a message NID.
    fn read_attachments(&self, message_nid: u32) -> Vec<PstAttachment> {
        let mut attachments = Vec::new();

        // Attachments are stored as sub-entries in the message's sub-node BTree.
        // Each attachment has NIDs in the range [message_nid + 1, message_nid + N].
        // Try to find attachment NIDs by scanning the NBT cache.
        for (&nid, _entry) in self.nbt_cache.iter() {
            // Attachment NIDs are typically close to the message NID.
            if nid <= message_nid || nid > message_nid + 1000 {
                continue;
            }

            // Check the MessageClass to see if it's an attachment.
            let msg_class = self.get_property_string(nid, PROP_TAG_MESSAGE_CLASS);
            if msg_class.as_deref() != Some("IPM.Attachment") {
                continue;
            }

            let name = self
                .get_property_string(nid, PROP_TAG_ATTACH_LONG_FILENAME)
                .or_else(|| {
                    self.get_property_string(nid, 0x3704) // PidTagAttachFilename
                })
                .unwrap_or_else(|| "unnamed".to_string());

            let attach_size_str = self.get_property_string(nid, PROP_TAG_ATTACH_SIZE);
            let size = attach_size_str
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            let mime_type = self
                .get_property_string(nid, PROP_TAG_ATTACH_MIME)
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let content_id = self.get_property_string(nid, 0x3712); // PidTagAttachContentId

            // Try to read binary attachment data.
            let data = self
                .get_property_binary(nid, PROP_TAG_ATTACH_DATA)
                .unwrap_or_default();

            attachments.push(PstAttachment {
                name,
                size,
                content_id,
                mime_type,
                data,
            });
        }

        attachments
    }

    /// Get a binary property value.
    fn get_property_binary(&self, nid: u32, prop_tag: u16) -> Option<Vec<u8>> {
        let block = self.read_subnode_block(nid)?;
        let props = self.parse_property_context(block);
        let tag = ((prop_tag as u32) << 16) | prop_type::PtypBinary as u32;
        if let Some(PropValue::Binary(data)) = props.get(&tag) {
            return Some(data.clone());
        }
        None
    }
}

fn split_display_addresses(raw: &str) -> Vec<String> {
    raw.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
