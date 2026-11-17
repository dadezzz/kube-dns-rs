use std::{
    net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr},
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
    #[error("the prefix length cannot be parsed: {0}")]
    InvalidPrefixLen(ParseIntError),
    #[error("the prefix length is too big for the address type specified")]
    InvalidPrefixLenSize,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct IpNet {
    addr: Ipv6Addr,
    mask_bits: u128,
}

impl IpNet {
    #[must_use]
    pub const fn new(addr: IpAddr, mut prefix_len: u32) -> Self {
        let addr = match addr {
            IpAddr::V4(ipv4_addr) => {
                prefix_len += Ipv6Addr::BITS - Ipv4Addr::BITS;
                ipv4_addr.to_ipv6_mapped()
            }
            IpAddr::V6(ipv6_addr) => ipv6_addr,
        };

        let mask_bits = !u128::MAX.unbounded_shr(prefix_len);
        // Mask the address here, to provide a canonical form that avoids
        // duplication in places like the KubernetesCrdContext by_net index.
        let addr = Ipv6Addr::from_bits(addr.to_bits() & mask_bits);

        Self { addr, mask_bits }
    }

    /// Returns `::/0`.
    #[must_use]
    pub const fn unspecified() -> Self {
        Self {
            addr: Ipv6Addr::UNSPECIFIED,
            mask_bits: 0,
        }
    }

    #[must_use]
    pub fn contains_host(&self, host: &IpAddr) -> bool {
        let host = match host {
            IpAddr::V4(ipv4_addr) => ipv4_addr.to_ipv6_mapped(),
            IpAddr::V6(ipv6_addr) => *ipv6_addr,
        };

        // Already masked in ::new().
        let net_bits = self.addr.to_bits();
        let host_bits = host.to_bits() & self.mask_bits;

        net_bits == host_bits
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
                    .map_or(Ok(Ipv4Addr::BITS), u32::from_str)
                    .map_err(IpNetError::InvalidPrefixLen)?;

                if prefix_len > Ipv4Addr::BITS {
                    return Err(IpNetError::InvalidPrefixLenSize);
                }

                prefix_len
            }
            IpAddr::V6(_) => {
                let prefix_len = prefix_len
                    .map_or(Ok(Ipv6Addr::BITS), u32::from_str)
                    .map_err(IpNetError::InvalidPrefixLen)?;

                if prefix_len > Ipv6Addr::BITS {
                    return Err(IpNetError::InvalidPrefixLenSize);
                }

                prefix_len
            }
        };

        Ok(IpNet::new(addr, prefix_len))
    }
}

impl From<IpNet> for String {
    fn from(value: IpNet) -> Self {
        format!("{}/{}", value.addr, value.mask_bits.count_ones())
    }
}

#[cfg(test)]
mod test {
    use std::{net::IpAddr, str::FromStr};

    use super::IpNet;

    #[test]
    fn any_network_ipv4() {
        let net = IpNet::try_from("1.1.1.1/0".to_owned()).unwrap();
        assert!(net.contains_host(&IpAddr::from_str("1.2.3.4").unwrap()));
        assert!(!net.contains_host(&IpAddr::from_str("1:3::").unwrap()));
        assert!(net.contains_host(&IpAddr::from_str("::ffff:1.2.3.4").unwrap()));
    }

    #[test]
    fn any_network() {
        let net = IpNet::try_from("::/0".to_owned()).unwrap();
        assert!(net.contains_host(&IpAddr::from_str("1.2.3.4").unwrap()));
        assert!(net.contains_host(&IpAddr::from_str("1:3::").unwrap()));
    }

    #[test]
    fn restricted_network() {
        let net = IpNet::try_from("192.168.0.0/16".to_owned()).unwrap();
        assert!(net.contains_host(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(!net.contains_host(&IpAddr::from_str("192.169.1.1").unwrap()));
    }
}
