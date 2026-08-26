use std::fmt::Write;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::identity::LOGICAL_SECTOR_SIZE;
use crate::{EmulationError, ParentIdentity};

const HEADS: u64 = 255;
const SECTORS_PER_TRACK: u64 = 63;
const MAX_CYLINDERS: u64 = 16_383;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmdkAdapter {
    Ide,
    LsiLogic,
}

impl VmdkAdapter {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ide => "ide",
            Self::LsiLogic => "lsilogic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmdkDescriptor {
    extent_path: String,
    sector_count: u64,
    cid: u32,
    adapter: VmdkAdapter,
}

impl VmdkDescriptor {
    pub fn new(
        parent: &ParentIdentity,
        extent_path: &str,
        adapter: VmdkAdapter,
    ) -> Result<Self, EmulationError> {
        validate_extent_path(extent_path)?;
        let sector_count = parent.logical_length() / LOGICAL_SECTOR_SIZE;
        let cid = descriptor_cid(parent, extent_path, adapter);
        Ok(Self {
            extent_path: extent_path.replace('/', "\\"),
            sector_count,
            cid,
            adapter,
        })
    }

    pub fn sector_count(&self) -> u64 {
        self.sector_count
    }

    pub fn parse(value: &str) -> Result<Self, EmulationError> {
        let cid = parse_cid(required_line_value(value, "CID=")?)?;
        if required_line_value(value, "parentCID=")? != "ffffffff"
            || required_line_value(value, "createType=")? != "\"monolithicFlat\""
        {
            return Err(invalid_descriptor("unsupported parent or create type"));
        }
        let (sector_count, extent_path) = parse_extent(value)?;
        let adapter = parse_adapter(required_line_value(value, "ddb.adapterType = ")?)?;
        validate_extent_path(&extent_path)?;
        Ok(Self {
            extent_path: extent_path.replace('/', "\\"),
            sector_count,
            cid,
            adapter,
        })
    }

    pub fn render(&self) -> String {
        let cylinders = self
            .sector_count
            .div_ceil(HEADS * SECTORS_PER_TRACK)
            .clamp(1, MAX_CYLINDERS);
        let mut output = String::new();
        writeln!(output, "# Disk DescriptorFile").expect("writing to a string cannot fail");
        writeln!(output, "version=1").expect("writing to a string cannot fail");
        writeln!(output, "encoding=\"UTF-8\"").expect("writing to a string cannot fail");
        writeln!(output, "CID={:08x}", self.cid).expect("writing to a string cannot fail");
        writeln!(output, "parentCID=ffffffff").expect("writing to a string cannot fail");
        writeln!(output, "createType=\"monolithicFlat\"\n")
            .expect("writing to a string cannot fail");
        writeln!(output, "# Extent description").expect("writing to a string cannot fail");
        writeln!(
            output,
            "RW {} FLAT \"{}\" 0\n",
            self.sector_count, self.extent_path
        )
        .expect("writing to a string cannot fail");
        writeln!(output, "# The Disk Data Base").expect("writing to a string cannot fail");
        writeln!(output, "ddb.adapterType = \"{}\"", self.adapter.as_str())
            .expect("writing to a string cannot fail");
        writeln!(output, "ddb.geometry.cylinders = \"{cylinders}\"")
            .expect("writing to a string cannot fail");
        writeln!(output, "ddb.geometry.heads = \"{HEADS}\"")
            .expect("writing to a string cannot fail");
        writeln!(output, "ddb.geometry.sectors = \"{SECTORS_PER_TRACK}\"")
            .expect("writing to a string cannot fail");
        output
    }
}

fn required_line_value<'a>(input: &'a str, prefix: &str) -> Result<&'a str, EmulationError> {
    let mut matches = input.lines().filter_map(|line| line.strip_prefix(prefix));
    let value = matches
        .next()
        .ok_or_else(|| invalid_descriptor(format!("missing {prefix} field")))?;
    if matches.next().is_some() {
        return Err(invalid_descriptor(format!("duplicate {prefix} field")));
    }
    Ok(value.trim())
}

fn parse_cid(value: &str) -> Result<u32, EmulationError> {
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_descriptor(
            "CID must contain eight hexadecimal digits",
        ));
    }
    u32::from_str_radix(value, 16).map_err(|_| invalid_descriptor("CID could not be decoded"))
}

fn parse_extent(input: &str) -> Result<(u64, String), EmulationError> {
    let mut matches = input.lines().filter(|line| line.starts_with("RW "));
    let line = matches
        .next()
        .ok_or_else(|| invalid_descriptor("missing flat extent"))?;
    if matches.next().is_some() {
        return Err(invalid_descriptor("multiple extents are not supported"));
    }
    let (sectors, remainder) = line[3..]
        .split_once(' ')
        .ok_or_else(|| invalid_descriptor("extent sector count is missing"))?;
    let sector_count = sectors
        .parse::<u64>()
        .map_err(|_| invalid_descriptor("extent sector count is invalid"))?;
    let extent_path = remainder
        .strip_prefix("FLAT \"")
        .and_then(|value| value.strip_suffix("\" 0"))
        .ok_or_else(|| invalid_descriptor("extent must be a zero-offset FLAT file"))?;
    if sector_count == 0 {
        return Err(invalid_descriptor("extent sector count must be non-zero"));
    }
    Ok((sector_count, extent_path.to_string()))
}

fn parse_adapter(value: &str) -> Result<VmdkAdapter, EmulationError> {
    match value {
        "\"ide\"" => Ok(VmdkAdapter::Ide),
        "\"lsilogic\"" => Ok(VmdkAdapter::LsiLogic),
        _ => Err(invalid_descriptor("unsupported VMDK adapter")),
    }
}

fn validate_extent_path(value: &str) -> Result<(), EmulationError> {
    if value.is_empty()
        || value.contains(['\r', '\n', '"'])
        || value.starts_with(['\\', '/'])
        || value.as_bytes().get(1) == Some(&b':')
    {
        return Err(EmulationError::InvalidExtentPath(value.to_string()));
    }
    let normalized = value.replace('\\', "/");
    if Path::new(&normalized)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(EmulationError::InvalidExtentPath(value.to_string()));
    }
    Ok(())
}

fn descriptor_cid(parent: &ParentIdentity, path: &str, adapter: VmdkAdapter) -> u32 {
    let mut digest = Sha256::new();
    digest.update(parent.sha256());
    digest.update(parent.logical_length().to_le_bytes());
    digest.update(path.as_bytes());
    digest.update([adapter as u8]);
    let hash = digest.finalize();
    u32::from_le_bytes(hash[..4].try_into().unwrap_or([0; 4]))
}

fn invalid_descriptor(message: impl Into<String>) -> EmulationError {
    EmulationError::InvalidVmdkDescriptor(message.into())
}
