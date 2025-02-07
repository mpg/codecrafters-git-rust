use anyhow::{anyhow, Result};

#[derive(Debug, PartialEq, Eq)]
pub enum ObjType {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl ObjType {
    pub fn from_bytes(label: &[u8]) -> Result<ObjType> {
        match label {
            b"commit" => Ok(ObjType::Commit),
            b"tree" => Ok(ObjType::Tree),
            b"blob" => Ok(ObjType::Blob),
            b"tag" => Ok(ObjType::Tag),
            l => Err(anyhow!("unknown object type {:?}", l)),
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            ObjType::Commit => "commit",
            ObjType::Tree => "tree",
            ObjType::Blob => "blob",
            ObjType::Tag => "tag",
        }
    }
}
