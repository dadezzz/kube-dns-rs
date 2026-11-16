use hickory_server::proto::rr::Name as HickoryName;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct Name {
    inner: HickoryName,
}

impl From<&Name> for HickoryName {
    fn from(value: &Name) -> Self {
        value.inner.clone()
    }
}

impl TryFrom<String> for Name {
    type Error = hickory_server::proto::ProtoError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self {
            inner: HickoryName::from_utf8(&value)?,
        })
    }
}

impl From<Name> for String {
    fn from(value: Name) -> Self {
        value.inner.to_utf8()
    }
}
