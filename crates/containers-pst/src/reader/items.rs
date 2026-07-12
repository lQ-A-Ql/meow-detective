use super::PstReader;
use crate::props::{PROP_TAG_MESSAGE_CLASS, PROP_TAG_SUBJECT};
use crate::{PstCalendar, PstContact, PstError};

impl PstReader {
    pub fn read_calendar(&self) -> Result<Vec<PstCalendar>, PstError> {
        let mut entries = Vec::new();
        for folder in self.collect_folder_nids()? {
            for nid in self.get_subnode_nids(folder)? {
                if self
                    .get_property_string(nid, PROP_TAG_MESSAGE_CLASS)
                    .as_deref()
                    != Some("IPM.Appointment")
                {
                    continue;
                }
                entries.push(PstCalendar {
                    subject: self
                        .get_property_string(nid, PROP_TAG_SUBJECT)
                        .unwrap_or_default(),
                    start_time: self.get_property_filetime(nid, 0x820d),
                    end_time: self.get_property_filetime(nid, 0x820e),
                    location: self.get_property_string(nid, 0x8208).unwrap_or_default(),
                    attendees: self
                        .get_property_string_array(nid, 0x823e)
                        .unwrap_or_default(),
                });
            }
        }
        Ok(entries)
    }

    pub fn read_contacts(&self) -> Result<Vec<PstContact>, PstError> {
        let mut contacts = Vec::new();
        for folder in self.collect_folder_nids()? {
            for nid in self.get_subnode_nids(folder)? {
                if self
                    .get_property_string(nid, PROP_TAG_MESSAGE_CLASS)
                    .as_deref()
                    != Some("IPM.Contact")
                {
                    continue;
                }
                contacts.push(self.read_contact(nid));
            }
        }
        Ok(contacts)
    }

    fn read_contact(&self, nid: u32) -> PstContact {
        PstContact {
            name: self.get_property_string(nid, 0x3a06).unwrap_or_default(),
            email: self
                .get_property_string(nid, 0x39fe)
                .or_else(|| self.get_property_string(nid, 0x8083))
                .unwrap_or_default(),
            phone: self
                .get_property_string(nid, 0x3a08)
                .or_else(|| self.get_property_string(nid, 0x3a1c))
                .unwrap_or_default(),
            address: self.get_property_string(nid, 0x3a29).unwrap_or_default(),
        }
    }
}
