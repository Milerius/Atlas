use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum IdError {
    #[error("id must not be empty")]
    Empty,
}

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Id);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                Id::new(value).map(Self)
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

typed_id!(ChainId);
typed_id!(NetworkId);
typed_id!(AssetGroupId);
typed_id!(AssetInstrumentId);
typed_id!(AssetInstanceId);
typed_id!(SignerId);
typed_id!(AccountRef);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_id_rejects_empty() {
        assert_eq!(
            NetworkId::new("").unwrap_err().to_string(),
            "id must not be empty"
        );
    }

    #[test]
    fn asset_instance_id_preserves_value() {
        let id = AssetInstanceId::new("eip155:8453/native:eth").unwrap();
        assert_eq!(id.as_str(), "eip155:8453/native:eth");
    }
}
