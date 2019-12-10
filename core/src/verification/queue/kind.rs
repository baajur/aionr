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

//! Definition of valid items for the verification queue.
use engine::Engine;
use types::error::Error;
use header::SealType;

use heapsize::HeapSizeOf;
use aion_types::{H256, U256};

pub use self::blocks::Blocks;
pub use self::headers::Headers;

/// Something which can produce a hash and a parent hash.
pub trait BlockLike {
    /// Get the hash of this item.
    fn hash(&self) -> H256;

    /// Get the hash of this item's parent.
    fn parent_hash(&self) -> H256;

    /// Get the difficulty of this item.
    fn difficulty(&self) -> U256;

    /// Get the seal_type of this item.
    fn seal_type(&self) -> &Option<SealType>;
}

/// Defines transitions between stages of verification.
///
/// It starts with a fallible transformation from an "input" into the unverified item.
/// This consists of quick, simply done checks as well as extracting particular data.
///
/// Then, there is a `verify` function which performs more expensive checks and
/// produces the verified output.
///
/// For correctness, the hashes produced by each stage of the pipeline should be
/// consistent.
pub trait Kind: 'static + Sized + Send + Sync {
    /// The first stage: completely unverified.
    type Input: Sized + Send + BlockLike + HeapSizeOf;

    /// The second stage: partially verified.
    type Unverified: Sized + Send + BlockLike + HeapSizeOf;

    /// The third stage: completely verified.
    type Verified: Sized + Send + BlockLike + HeapSizeOf;

    /// Attempt to create the `Unverified` item from the input.
    fn create(input: Self::Input, engine: &dyn Engine) -> Result<Self::Unverified, Error>;

    /// Attempt to verify the `Unverified` item using the given engine.
    fn verify(unverified: Self::Unverified, engine: &dyn Engine) -> Result<Self::Verified, Error>;
}

/// The blocks verification module.
pub mod blocks {
    use super::{Kind, BlockLike};

    use engine::Engine;
    use types::error::{Error, BlockError};
    use header::{Header,SealType};
    use verification::{PreverifiedBlock, verify_block_basic, verify_block_unordered};

    use heapsize::HeapSizeOf;
    use aion_types::{H256, U256};
    use acore_bytes::Bytes;

    /// A mode for verifying blocks.
    pub struct Blocks;

    impl Kind for Blocks {
        type Input = Unverified;
        type Unverified = Unverified;
        type Verified = PreverifiedBlock;

        fn create(input: Self::Input, engine: &dyn Engine) -> Result<Self::Unverified, Error> {
            match verify_block_basic(&input.header, &input.bytes, engine) {
                Ok(()) => Ok(input),
                Err(Error::Block(BlockError::TemporarilyInvalid(oob))) => {
                    debug!(target: "client", "Block received too early {}: {:?}", input.hash(), oob);
                    Err(BlockError::TemporarilyInvalid(oob).into())
                }
                Err(e) => {
                    warn!(target: "client", "Stage 1 block verification failed for {}: {:?}", input.hash(), e);
                    Err(e)
                }
            }
        }

        fn verify(un: Self::Unverified, engine: &dyn Engine) -> Result<Self::Verified, Error> {
            let hash = un.hash();
            match verify_block_unordered(un.header, un.bytes, engine) {
                Ok(verified) => Ok(verified),
                Err(e) => {
                    warn!(target: "client", "Stage 2 block verification failed for {}: {:?}", hash, e);
                    Err(e)
                }
            }
        }
    }

    /// An unverified block.
    pub struct Unverified {
        header: Header,
        bytes: Bytes,
    }

    impl Unverified {
        /// Create an `Unverified` from raw bytes.
        pub fn new(bytes: Bytes) -> Self {
            use views::BlockView;

            let header = BlockView::new(&bytes).header();
            Unverified {
                header,
                bytes,
            }
        }
    }

    impl HeapSizeOf for Unverified {
        fn heap_size_of_children(&self) -> usize {
            self.header.heap_size_of_children() + self.bytes.heap_size_of_children()
        }
    }

    impl BlockLike for Unverified {
        fn hash(&self) -> H256 { self.header.hash() }

        fn parent_hash(&self) -> H256 { self.header.parent_hash().clone() }

        fn difficulty(&self) -> U256 { self.header.difficulty().clone() }

        fn seal_type(&self) -> &Option<SealType> { self.header.seal_type() }
    }

    impl BlockLike for PreverifiedBlock {
        fn hash(&self) -> H256 { self.header.hash() }

        fn parent_hash(&self) -> H256 { self.header.parent_hash().clone() }

        fn difficulty(&self) -> U256 { self.header.difficulty().clone() }

        fn seal_type(&self) -> &Option<SealType> { self.header.seal_type() }
    }
}

/// Verification for headers.
pub mod headers {
    use super::{Kind, BlockLike};

    use engine::Engine;
    use types::error::Error;
    use header::{Header,SealType};
    use verification::verify_header_params;

    use aion_types::{H256, U256};

    impl BlockLike for Header {
        fn hash(&self) -> H256 { self.hash() }
        fn parent_hash(&self) -> H256 { self.parent_hash().clone() }
        fn difficulty(&self) -> U256 { self.difficulty().clone() }
        fn seal_type(&self) -> &Option<SealType> { self.seal_type() }
    }

    /// A mode for verifying headers.
    pub struct Headers;

    impl Kind for Headers {
        type Input = Header;
        type Unverified = Header;
        type Verified = Header;

        fn create(input: Self::Input, engine: &dyn Engine) -> Result<Self::Unverified, Error> {
            verify_header_params(&input, engine, true).map(|_| input)
        }

        fn verify(unverified: Self::Unverified, engine: &dyn Engine) -> Result<Self::Verified, Error> {
            engine
                .verify_block_unordered(&unverified)
                .map(|_| unverified)
        }
    }
}
