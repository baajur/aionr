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

//! View onto block body rlp.

use aion_types::H256;
use blake2b::blake2b;
use header::BlockNumber;
use rlp::Rlp;
use transaction::{LocalizedTransaction, UnverifiedTransaction};
use views::TransactionView;

/// View onto block rlp.
pub struct BodyView<'a> {
    rlp: Rlp<'a>,
}

impl<'a> BodyView<'a> {
    /// Creates new view onto block from raw bytes.
    pub fn new(bytes: &'a [u8]) -> BodyView<'a> {
        BodyView {
            rlp: Rlp::new(bytes),
        }
    }

    /// Creates new view onto block from rlp.
    pub fn new_from_rlp(rlp: Rlp<'a>) -> BodyView<'a> {
        BodyView {
            rlp: rlp,
        }
    }

    /// Return reference to underlaying rlp.
    pub fn rlp(&self) -> &Rlp<'a> { &self.rlp }

    /// Return List of transactions in given block.
    pub fn transactions(&self) -> Vec<UnverifiedTransaction> { self.rlp.list_at(0) }

    /// Return List of transactions with additional localization info.
    pub fn localized_transactions(
        &self,
        block_hash: &H256,
        block_number: BlockNumber,
    ) -> Vec<LocalizedTransaction>
    {
        self.transactions()
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                LocalizedTransaction {
                    signed: t,
                    block_hash: block_hash.clone(),
                    block_number: block_number,
                    transaction_index: i,
                    cached_sender: None,
                }
            })
            .collect()
    }

    /// Return number of transactions in given block, without deserializing them.
    pub fn transactions_count(&self) -> usize { self.rlp.at(0).item_count() }

    /// Return List of transactions in given block.
    pub fn transaction_views(&self) -> Vec<TransactionView<'a>> {
        self.rlp
            .at(0)
            .iter()
            .map(TransactionView::new_from_rlp)
            .collect()
    }

    /// Return transaction hashes.
    pub fn transaction_hashes(&self) -> Vec<H256> {
        self.rlp
            .at(0)
            .iter()
            .map(|rlp| blake2b(rlp.as_raw()))
            .collect()
    }

    /// Returns transaction at given index without deserializing unnecessary data.
    pub fn transaction_at(&self, index: usize) -> Option<UnverifiedTransaction> {
        self.rlp.at(0).iter().nth(index).map(|rlp| rlp.as_val())
    }

    /// Returns localized transaction at given index.
    pub fn localized_transaction_at(
        &self,
        block_hash: &H256,
        block_number: BlockNumber,
        index: usize,
    ) -> Option<LocalizedTransaction>
    {
        self.transaction_at(index).map(|t| {
            LocalizedTransaction {
                signed: t,
                block_hash: block_hash.clone(),
                block_number: block_number,
                transaction_index: index,
                cached_sender: None,
            }
        })
    }
}
