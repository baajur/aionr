
use std::sync::Arc;
use std::collections::{HashMap};
use blake2b::{BLAKE2B_EMPTY, BLAKE2B_NULL_RLP, blake2b};
use aion_types::{H128, H256, U256, Address};
use bytes::{Bytes, ToPretty};
use trie;
use trie::{SecTrieDB, Trie, TrieFactory};
use lru_cache::LruCache;
use basic_account::BasicAccount;
use kvdb::{DBValue, HashStore};

use rlp::encode;

use std::cell::{RefCell, Cell};
use super::{RequireCache, AccountState};

const STORAGE_CACHE_ITEMS: usize = 8192;

/// Boolean type for clean/dirty status.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Filth {
    /// Data has not been changed.
    Clean,
    /// Data has been changed.
    Dirty,
}

#[derive(Clone, Debug)]
pub struct AVMAccount {
    // Balance of the account.
    balance: U256,
    // Nonce of the account.
    nonce: U256,
    // Trie-backed storage.
    storage_root: H256,
    // LRU Cache of the trie-backed storage.
    // This is limited to `STORAGE_CACHE_ITEMS` recent queries
    storage_cache: RefCell<LruCache<Bytes, Bytes>>,
    // Modified storage. Accumulates changes to storage made in `set_storage`
    // Takes precedence over `storage_cache`.
    storage_changes: HashMap<Bytes, Bytes>,

    // Code hash of the account.
    code_hash: H256,
    // Size of the accoun code.
    code_size: Option<usize>,
    // Code cache of the account.
    code_cache: Arc<Bytes>,
    // Account code new or has been modified.
    code_filth: Filth,
    // Cached address hash.
    address_hash: Cell<Option<H256>>,
}

impl From<BasicAccount> for AVMAccount {
    fn from(basic: BasicAccount) -> Self {
        AVMAccount {
            balance: basic.balance,
            nonce: basic.nonce,
            storage_root: basic.storage_root,
            storage_cache: Self::empty_storage_cache(),
            storage_changes: HashMap::new(),
            code_hash: basic.code_hash,
            code_size: None,
            code_cache: Arc::new(vec![]),
            code_filth: Filth::Clean,
            address_hash: Cell::new(None),
        }
    }
}

#[derive(Debug)]
pub struct AVMAccountEntry {
    pub account: Option<AVMAccount>,
    pub old_balance: Option<U256>,
    pub state: AccountState,
}

impl AVMAccountEntry {
    pub fn new_clean(account: Option<AVMAccount>) -> AVMAccountEntry {
        AVMAccountEntry {
            old_balance: account.as_ref().map(|a| a.balance().clone()),
            account: account,
            state: AccountState::CleanFresh,
        }
    }

    pub fn new_dirty(account: Option<AVMAccount>) -> AVMAccountEntry {
        AVMAccountEntry {
            old_balance: account.as_ref().map(|a| a.balance().clone()),
            account: account,
            state: AccountState::Dirty,
        }
    }

    // Create a new account entry and mark it as clean and cached.
    pub fn new_clean_cached(account: Option<AVMAccount>) -> AVMAccountEntry {
        AVMAccountEntry {
            old_balance: account.as_ref().map(|a| a.balance().clone()),
            account: account,
            state: AccountState::CleanCached,
        }
    }

    pub fn get_state(&self) -> AccountState {
        self.state.clone()
    }

    pub fn change_state(&mut self, state: AccountState) {
        self.state = state;
    }

    pub fn is_dirty(&self) -> bool { self.state == AccountState::Dirty }
}

impl AVMAccount
{
    /// Create a new account with the given balance.
    pub fn new_basic(balance: U256, nonce: U256) -> AVMAccount {
        AVMAccount {
            balance: balance,
            nonce: nonce,
            storage_root: BLAKE2B_NULL_RLP,
            storage_cache: Self::empty_storage_cache(),
            storage_changes: HashMap::new(),
            code_hash: BLAKE2B_EMPTY,
            code_cache: Arc::new(vec![]),
            code_size: Some(0),
            code_filth: Filth::Clean,
            address_hash: Cell::new(None),
        }
    }

    /// Create a new account from RLP.
    pub fn from_rlp(rlp: &[u8]) -> AVMAccount {
        let basic: BasicAccount = ::rlp::decode(rlp);
        basic.into()
    }

    pub fn balance(&self) -> &U256 {
        return &self.balance;
    }

    /// return the nonce associated with this account.
    pub fn nonce(&self) -> &U256 { &self.nonce }

    /// Increment the nonce of the account by one.
    pub fn inc_nonce(&mut self) { self.nonce = self.nonce + U256::from(1u8); }

    /// Increase account balance.
    pub fn add_balance(&mut self, x: &U256) { self.balance = self.balance + *x; }

    /// Decrease account balance.
    /// Panics if balance is less than `x`
    pub fn sub_balance(&mut self, x: &U256) {
        println!("balance = {:?}, decrement = {:?}", self.balance, *x);
        assert!(self.balance >= *x);
        self.balance = self.balance - *x;
    }

    /// Set this account's code to the given code.
    /// NOTE: Account should have been created with `new_contract()`
    pub fn init_code(&mut self, code: Bytes) {
        self.code_hash = blake2b(&code);
        self.code_cache = Arc::new(code);
        self.code_size = Some(self.code_cache.len());
        self.code_filth = Filth::Dirty;
    }

    /// returns the account's code. If `None` then the code cache isn't available -
    /// get someone who knows to call `note_code`.
    pub fn code(&self) -> Option<Arc<Bytes>> {
        // [FZH] to understand why 'self.code_hash != BLAKE2B_EMPTY'
        // if self.code_hash != BLAKE2B_EMPTY && self.code_cache.is_empty() {
        if self.code_cache.is_empty() {
            return None;
        }

        Some(self.code_cache.clone())
    }

    pub fn cache_given_code(&mut self, code: Arc<Bytes>) {
        trace!(
            target: "account",
            "Account::cache_given_code: ic={}; self.code_hash={:?}, self.code_cache={}",
            self.is_cached(),
            self.code_hash,
            self.code_cache.pretty()
        );

        self.code_size = Some(code.len());
        self.code_cache = code;
    }

    /// Provide a database to get `code_size`. Should not be called if it is a contract without code.
    pub fn cache_code_size(&mut self, db: &HashStore) -> bool {
        // TODO: fill out self.code_cache;
        trace!(
            target: "account",
            "Account::cache_code_size: ic={}; self.code_hash={:?}, self.code_cache={}",
            self.is_cached(),
            self.code_hash,
            self.code_cache.pretty()
        );
        self.code_size.is_some() || if self.code_hash != BLAKE2B_EMPTY {
            match db.get(&self.code_hash) {
                Some(x) => {
                    self.code_size = Some(x.len());
                    true
                }
                _ => {
                    warn!(target: "account","Failed reverse get of {}", self.code_hash);
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn address_hash(&self, address: &Address) -> H256 {
        let hash = self.address_hash.get();
        hash.unwrap_or_else(|| {
            let hash = blake2b(address);
            self.address_hash.set(Some(hash.clone()));
            hash
        })
    }

    pub fn code_hash(&self) -> H256 { self.code_hash.clone() }

    pub fn is_cached(&self) -> bool {
        !self.code_cache.is_empty()
            || (self.code_cache.is_empty() && self.code_hash == BLAKE2B_EMPTY)
    }

    pub fn cache_code(&mut self, db: &HashStore) -> Option<Arc<Bytes>> {
        // TODO: fill out self.code_cache;
        trace!(
            target: "account",
            "Account::cache_code: ic={}; self.code_hash={:?}, self.code_cache={}",
            self.is_cached(),
            self.code_hash,
            self.code_cache.pretty()
        );

        if self.is_cached() {
            return Some(self.code_cache.clone());
        }

        match db.get(&self.code_hash) {
            Some(x) => {
                self.code_size = Some(x.len());
                self.code_cache = Arc::new(x.into_vec());
                Some(self.code_cache.clone())
            }
            _ => {
                warn!(target: "account","Failed reverse get of {}", self.code_hash);
                None
            }
        }
    }

    pub fn cached_storage_at(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
        println!("search storage_changes: {:?}", self.storage_changes);
        if let Some(value) = self.storage_changes.get(key) {
            return Some(value.clone());
        }
        if let Some(value) = self.storage_cache.borrow_mut().get_mut(key) {
            return Some(value.clone());
        }
        None
    }

    fn empty_storage_cache() -> RefCell<LruCache<Vec<u8>, Vec<u8>>> {
        RefCell::new(LruCache::new(STORAGE_CACHE_ITEMS))
    }

    pub fn storage_at(&self, db: &HashStore, key: &Vec<u8>) -> trie::Result<Vec<u8>> {
        println!("get storage: key = {:?}", key);
        if let Some(value) = self.cached_storage_at(key) {
            return Ok(value);
        }
        let db = SecTrieDB::new(db, &self.storage_root)?;

        let value: Vec<u8> = db.get_with(key, ::rlp::decode)?.unwrap_or_else(|| vec![]);
        self.storage_cache
            .borrow_mut()
            .insert(key.clone(), value.clone());
        println!("get storage value from db: key = {:?}, value = {:?}", key, value);
        Ok(value)
    }

    /// Set (and cache) the contents of the trie's storage at `key` to `value`.
    pub fn set_storage(&mut self, key: Bytes, value: Bytes) {
        println!("pre storage_changes = {:?}", self.storage_changes);
        self.storage_changes.insert(key, value);
        let raw_changes: *mut HashMap<Vec<u8>, Vec<u8>> = unsafe {::std::mem::transmute(&self.storage_changes)};
        println!("storage_changes ptr = {:?}", raw_changes);
        println!("post storage_changes = {:?}", self.storage_changes);
    }

    /// Clone basic account data
    pub fn clone_basic(&self) -> AVMAccount {
        AVMAccount {
            balance: self.balance.clone(),
            nonce: self.nonce.clone(),
            storage_root: self.storage_root.clone(),
            storage_cache: Self::empty_storage_cache(),
            storage_changes: HashMap::new(),
            code_hash: self.code_hash.clone(),
            code_size: self.code_size.clone(),
            code_cache: self.code_cache.clone(),
            code_filth: self.code_filth,
            address_hash: self.address_hash.clone(),
        }
    }

    /// Create a new contract account.
    /// NOTE: make sure you use `init_code` on this before `commit`ing.
    pub fn new_contract(balance: U256, nonce: U256) -> AVMAccount {
        AVMAccount {
            balance: balance,
            nonce: nonce,
            storage_root: BLAKE2B_NULL_RLP,
            storage_cache: Self::empty_storage_cache(),
            storage_changes: HashMap::new(),
            code_hash: BLAKE2B_EMPTY,
            code_cache: Arc::new(vec![]),
            code_size: None,
            code_filth: Filth::Clean,
            address_hash: Cell::new(None),
        }
    }

    /// Commit any unsaved code. `code_hash` will always return the hash of the `code_cache` after this.
    pub fn commit_code(&mut self, db: &mut HashStore) {
        trace!(
            target: "account",
            "Commiting code of {:?} - {:?}, {:?}",
            self,
            self.code_filth == Filth::Dirty,
            self.code_cache.is_empty()
        );
        match (self.code_filth == Filth::Dirty, self.code_cache.is_empty()) {
            (true, true) => {
                self.code_size = Some(0);
                self.code_filth = Filth::Clean;
            }
            (true, false) => {
                db.emplace(
                    self.code_hash.clone(),
                    DBValue::from_slice(&*self.code_cache),
                );
                self.code_size = Some(self.code_cache.len());
                self.code_filth = Filth::Clean;
            }
            (false, _) => {}
        }
    }

    /// Check if account has zero nonce, balance, no code.
    pub fn is_null(&self) -> bool {
        self.balance.is_zero() && self.nonce.is_zero() && self.code_hash == BLAKE2B_EMPTY
    }

    /// Determine whether there are any un-`commit()`-ed storage-setting operations.
    pub fn storage_is_clean(&self) -> bool {
        self.storage_changes.is_empty()
    }

    /// Check if account has zero nonce, balance, no code and no storage.
    ///
    /// NOTE: Will panic if `!self.storage_is_clean()`
    pub fn is_empty(&self) -> bool {
        assert!(
            self.storage_is_clean(),
            "Account::is_empty() may only legally be called when storage is clean."
        );
        self.is_null() && self.storage_root == BLAKE2B_NULL_RLP
    }

    /// Commit the `storage_changes` to the backing DB and update `storage_root`.
    pub fn commit_storage(
        &mut self,
        trie_factory: &TrieFactory,
        db: &mut HashStore,
    ) -> trie::Result<()>
    {
        let mut t = trie_factory.from_existing(db, &mut self.storage_root)?;
        for (k, v) in self.storage_changes.drain() {
            // cast key and value to trait type,
            // so we can call overloaded `to_bytes` method
            let mut is_zero = true;
            for item in &v {
                if *item != 0x00 {
                    is_zero = false;
                    break;
                }
            }
            match is_zero {
                true => t.remove(&k)?,
                false => t.insert(&k, &encode(&v))?,
            };

            self.storage_cache.borrow_mut().insert(k, v);
        }
        Ok(())
    }

    pub fn discard_storage_changes(&mut self) {
        self.storage_changes.clear();
    }

    /// Return the storage overlay.
    pub fn storage_changes(&self) -> &HashMap<Bytes, Bytes> { &self.storage_changes }
}

pub struct AVMAccMgr {
    pub cache: RefCell<HashMap<Address, AVMAccountEntry>>,
    pub checkpoints: RefCell<Vec<HashMap<Address, Option<AVMAccountEntry>>>>,
}

impl AVMAccMgr {
    pub fn new() -> Self {
        AVMAccMgr {
            cache: RefCell::new(HashMap::new()),
            checkpoints: RefCell::new(Vec::new()),
        }
    }
    pub fn new_account(&mut self, address: &Address) {
        self.cache.borrow_mut().insert(*address, AVMAccountEntry::new_dirty(Some(AVMAccount::new_basic(0.into(), 0.into()))));
    }

    pub fn insert_cache(&self, address: &Address, account: AVMAccountEntry) {
        self.cache.borrow_mut().insert(*address, account);
    }

    pub fn note_cache(&self, _address: &Address) {
        //TODO: whether we need a checkpoint to revert account code
        // unimplemented!()
    }

    /// Remove an existing account.
    pub fn kill_account(&mut self, account: &Address) {
        self.insert_cache(account, AVMAccountEntry::new_dirty(None));
    }
}

pub trait AVMInterface {
    fn new_avm_account(&mut self, a: &Address) -> trie::Result<()>;
    fn check_avm_acc_exists(&self, a: &Address) -> trie::Result<bool>;
    fn set_avm_storage(&mut self, a: &Address, key: Vec<u8>, value: Vec<u8>) -> trie::Result<()>;
    fn get_avm_storage(&self, a: &Address, key: &Vec<u8>) -> trie::Result<Vec<u8>>;
    fn remove_avm_account(&mut self, a: &Address) -> trie::Result<()>;
    fn ensure_avm_cached<F, U>(
        &self,
        a: &Address,
        require: RequireCache,
        check_null: bool,
        f: F,
    ) -> trie::Result<U>
    where
        F: Fn(Option<&AVMAccount>) -> U;
}