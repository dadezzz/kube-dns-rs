use std::{
    net::{AddrParseError, IpAddr},
    num::ParseIntError,
    str::FromStr,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum IpNetError {
    #[error("the CIDR string is malformed")]
    MalformedString,
    #[error("the address cannot be parsed: {0}")]
    InvalidAddress(AddrParseError),
    #[error("the prefix lenght cannot be parsed: {0}")]
    InvalidPrefixLen(ParseIntError),
    #[error("the prefix length is too big for the address type specified")]
    InvalidPrefixLenSize,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct IpNet {
    addr: IpAddr,
    prefix_len: u32,
}

impl IpNet {
    #[must_use]
    pub fn new(addr: IpAddr, prefix_len: u32) -> Self {
        Self { addr, prefix_len }
    }

    #[must_use]
    pub fn contains_host(&self, host: &IpAddr) -> bool {
        match (self.addr.to_canonical(), host.to_canonical()) {
            (IpAddr::V4(net_addr), IpAddr::V4(host_addr)) => {
                let mask_bits = !u32::MAX.unbounded_shr(self.prefix_len);
                let net_bits = net_addr.to_bits() & mask_bits;
                let host_bits = host_addr.to_bits() & mask_bits;

                net_bits == host_bits
            }
            (IpAddr::V6(net_addr), IpAddr::V6(host_addr)) => {
                let mask_bits = !u128::MAX.unbounded_shr(self.prefix_len);
                let net_bits = net_addr.to_bits() & mask_bits;
                let host_bits = host_addr.to_bits() & mask_bits;

                net_bits == host_bits
            }
            _ => false,
        }
    }
}

impl TryFrom<String> for IpNet {
    type Error = IpNetError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut parts = value.split('/');
        let addr = parts.next().ok_or(IpNetError::MalformedString)?;
        let prefix_len = parts.next();

        if parts.next().is_some() {
            return Err(IpNetError::MalformedString);
        }

        let addr = IpAddr::from_str(addr).map_err(IpNetError::InvalidAddress)?;

        let prefix_len = match addr {
            IpAddr::V4(_) => {
                let prefix_len = prefix_len
                    .map_or(Ok(0), u32::from_str)
                    .map_err(IpNetError::InvalidPrefixLen)?;

                if prefix_len > 32 {
                    return Err(IpNetError::InvalidPrefixLenSize);
                }

                prefix_len
            }
            IpAddr::V6(_) => {
                let prefix_len = prefix_len
                    .map_or(Ok(0), u32::from_str)
                    .map_err(IpNetError::InvalidPrefixLen)?;

                if prefix_len > 128 {
                    return Err(IpNetError::InvalidPrefixLenSize);
                }

                prefix_len
            }
        };

        Ok(IpNet { addr, prefix_len })
    }
}

impl From<IpNet> for String {
    fn from(value: IpNet) -> Self {
        format!("{}/{}", value.addr, value.prefix_len)
    }
}

#[cfg(test)]
mod test {
    use std::{net::IpAddr, str::FromStr};

    use super::IpNet;

    #[test]
    fn any_network() {
        let net = IpNet::try_from("1.1.1.1/0".to_owned()).unwrap();
        assert!(net.contains_host(&IpAddr::from_str("1.2.3.4").unwrap()));
        assert!(!net.contains_host(&IpAddr::from_str("1:3::").unwrap()));
    }

    #[test]
    fn restricted_network() {
        let net = IpNet::try_from("192.168.0.0/16".to_owned()).unwrap();
        assert!(net.contains_host(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(!net.contains_host(&IpAddr::from_str("192.169.1.1").unwrap()));
    }
}
