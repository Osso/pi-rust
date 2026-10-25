//! Private frozen Map ordering: updates retain a slot; deletion/reinsertion gets
//! a new slot. Live refresh cursors skip deletions and visit new insertions.
use super::{ProviderConfigInput, upsert_provider_config};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Bound::{Excluded, Unbounded},
};

#[derive(Default)]
pub(super) struct RegisteredProviders {
    configs: BTreeMap<u64, (String, ProviderConfigInput)>,
    names: HashMap<String, u64>,
    next_slot: u64,
}
impl RegisteredProviders {
    pub(super) fn get(&self, provider: &str) -> Option<&ProviderConfigInput> {
        self.names.get(provider).map(|slot| &self.configs[slot].1)
    }
    pub(super) fn upsert(&mut self, provider: &str, config: ProviderConfigInput) {
        if let Some(slot) = self.names.get(provider) {
            let existing = &mut self.configs.get_mut(slot).expect("registered provider slot").1;
            upsert_provider_config(existing, config);
            return;
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.names.insert(provider.into(), slot);
        self.configs.insert(slot, (provider.into(), config));
    }
    pub(super) fn remove(&mut self, provider: &str) -> bool {
        let Some(slot) = self.names.remove(provider) else {
            return false;
        };
        self.configs.remove(&slot).is_some()
    }
    pub(super) fn next_after(&self, cursor: Option<u64>) -> Option<(u64, String, ProviderConfigInput)> {
        self.configs
            .range((cursor.map_or(Unbounded, Excluded), Unbounded))
            .next()
            .map(|(slot, (provider, config))| (*slot, provider.clone(), config.clone()))
    }
}
