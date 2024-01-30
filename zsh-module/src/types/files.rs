use std::{fs::DirEntry, path::*, str::FromStr};

use crate::{ToCString, Zerror};

/// A helper struct to represent an owned filepath
///
/// Caches the internal path, as well as the display string and its character length.
///
/// All methods for creating this type will check if the filepath exists, and fail if it does not, unless otherwise specified.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilePath {
    /// The path of the file
    pub path: PathBuf,
    /// The display string
    pub string: String,
    /// The length of this path in characters
    pub length: usize,
}
impl FilePath {
    /// Create a new, owned, checked, filepath. This is the preferred way to create this type.
    pub fn new<P>(pathlike: P) -> Result<Self, Zerror>
    where
        P: AsRef<Path>,
    {
        let path = pathlike.as_ref().to_path_buf();
        if !path.exists() {
            return Err(Zerror::FileNotFound(path));
        }

        let string = path.to_string_lossy().to_string();
        let length = string.chars().count();
        Ok(Self {
            path,
            string,
            length,
        })
    }
    /// Create a new instance of self WITHOUT checking if the path exists. Use with caution.
    pub fn new_unchecked<P>(pathlike: P) -> Self
    where
        P: AsRef<Path>,
    {
        let path = pathlike.as_ref().to_path_buf();
        let string = path.to_string_lossy().to_string();
        let length = string.chars().count();
        Self {
            path,
            string,
            length,
        }
    }
    /// Set this filepath's value
    pub fn set<P>(mut self, new_pathlike_value: P) -> Result<(), Zerror>
    where
        P: AsRef<Path>,
    {
        self = Self::new(new_pathlike_value)?;
        Ok(())
    }
}

impl std::fmt::Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.string.fmt(f)
    }
}
impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}
impl AsRef<str> for FilePath {
    fn as_ref(&self) -> &str {
        self.string.as_str()
    }
}
impl FromStr for FilePath {
    type Err = Zerror;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}
impl TryFrom<PathBuf> for FilePath {
    type Error = Zerror;
    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl ToCString for FilePath {
    fn into_cstr<'a>(self) -> std::borrow::Cow<'a, std::ffi::CStr>
    where
        Self: 'a,
    {
        self.string.into_cstr()
    }
}
impl TryFrom<DirEntry> for FilePath {
    type Error = Zerror;
    fn try_from(d: DirEntry) -> Result<Self, Self::Error> {
        Self::new(d.path())
    }
}