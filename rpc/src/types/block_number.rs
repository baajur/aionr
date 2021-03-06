/*******************************************************************************
 * Copyright (c) 2015-2018 Parity Technologies (UK) Ltd.
 * Copyright (c) 2018-2019 Aion foundation.
 *
 *     This file is part of the aion network project.
 *
 *     The aion network project is free software: you can redistribute it
 *     and/or modify it under the terms of the GNU General Public License
 *     as published by the Free Software Foundation, either version 3 of
 *     the License, or any later version.
 *
 *     The aion network project is distributed in the hope that it will
 *     be useful, but WITHOUT ANY WARRANTY; without even the implied
 *     warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 *     See the GNU General Public License for more details.
 *
 *     You should have received a copy of the GNU General Public License
 *     along with the aion network project source files.
 *     If not, see <https://www.gnu.org/licenses/>.
 *
 ******************************************************************************/

use std::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{Error, Visitor};
use acore::client::BlockId;

/// Represents rpc api block number param.
#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub enum BlockNumber {
    /// Number
    Num(u64),
    /// Latest block
    Latest,
    /// Earliest block (genesis)
    Earliest,
    /// Pending block (being mined)
    Pending,
}

impl Default for BlockNumber {
    fn default() -> Self { BlockNumber::Latest }
}

impl<'a> Deserialize<'a> for BlockNumber {
    fn deserialize<D>(deserializer: D) -> Result<BlockNumber, D::Error>
    where D: Deserializer<'a> {
        deserializer.deserialize_any(BlockNumberVisitor)
    }
}

impl BlockNumber {
    /// Convert block number to min block target.
    pub fn to_min_block_num(&self) -> Option<u64> {
        match *self {
            BlockNumber::Num(ref x) => Some(*x),
            _ => None,
        }
    }
}

impl Serialize for BlockNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match *self {
            BlockNumber::Num(ref x) => serializer.serialize_str(&format!("0x{:x}", x)),
            BlockNumber::Latest => serializer.serialize_str("latest"),
            BlockNumber::Earliest => serializer.serialize_str("earliest"),
            BlockNumber::Pending => serializer.serialize_str("pending"),
        }
    }
}

struct BlockNumberVisitor;

impl<'a> Visitor<'a> for BlockNumberVisitor {
    type Value = BlockNumber;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "a block number or 'latest', 'earliest' or 'pending'"
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where E: Error {
        match value {
            "latest" => Ok(BlockNumber::Latest),
            "earliest" => Ok(BlockNumber::Earliest),
            "pending" => Ok(BlockNumber::Pending),
            // support both "0x<hex>" and "<decimal>" format
            _ if value.starts_with("0x") => {
                u64::from_str_radix(&value[2..], 16)
                    .map(BlockNumber::Num)
                    .map_err(|e| Error::custom(format!("Invalid block number: {}", e)))
            }
            _ => {
                u64::from_str_radix(&value, 10)
                    .map(BlockNumber::Num)
                    .map_err(|e| Error::custom(format!("Invalid block number: {}", e)))
            }
        }
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where E: Error {
        self.visit_str(value.as_ref())
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where E: Error {
        // support decimal format.
        Ok(BlockNumber::Num(v))
    }
}

impl Into<BlockId> for BlockNumber {
    fn into(self) -> BlockId {
        match self {
            BlockNumber::Num(n) => BlockId::Number(n),
            BlockNumber::Earliest => BlockId::Earliest,
            BlockNumber::Latest => BlockId::Latest,
            BlockNumber::Pending => BlockId::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use acore::client::BlockId;
    use super::*;
    use serde_json;

    #[test]
    fn block_number_deserialization() {
        let s = r#"["0xa", "latest", "earliest", "pending"]"#;
        let deserialized: Vec<BlockNumber> = serde_json::from_str(s).unwrap();
        assert_eq!(
            deserialized,
            vec![
                BlockNumber::Num(10),
                BlockNumber::Latest,
                BlockNumber::Earliest,
                BlockNumber::Pending,
            ]
        )
    }

    #[test]
    fn should_not_deserialize_decimal() {
        let s = r#""10""#;
        // now support decimal
        assert!(serde_json::from_str::<BlockNumber>(s).is_ok());
    }

    #[test]
    fn block_number_into() {
        assert_eq!(BlockId::Number(100), BlockNumber::Num(100).into());
        assert_eq!(BlockId::Earliest, BlockNumber::Earliest.into());
        assert_eq!(BlockId::Latest, BlockNumber::Latest.into());
        assert_eq!(BlockId::Pending, BlockNumber::Pending.into());
    }
}
