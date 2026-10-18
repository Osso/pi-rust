//! Port of `packages/agent/src/harness/session/state.ts`.

use std::collections::{HashMap, HashSet};

use crate::harness::session::types::{
    BranchBounds, Entry, EntryOrder, EntryQuery, EntryType, FactItem, ForkOptions, LanePointer, LaneRecord, LogItem,
    LogOptions, OperationKind, RecordQuery, RecordType, SessionError, SessionErrorCode, SessionMutation, SessionStats,
    StorageBranchQuery,
};

fn invalid_mutation(message: impl AsRef<str>) -> SessionError {
    SessionError::new(
        SessionErrorCode::InvalidEntry,
        format!("Invalid session mutation: {}", message.as_ref()),
    )
}

/// Validate a positive query limit.
pub fn assert_valid_limit(limit: Option<usize>) -> Result<(), SessionError> {
    match limit {
        Some(0) => Err(SessionError::new(
            SessionErrorCode::InvalidQuery,
            "limit must be a positive integer",
        )),
        _ => Ok(()),
    }
}

/// Validate a non-negative cursor sequence.
pub fn assert_valid_cursor(after_seq: Option<i64>) -> Result<(), SessionError> {
    match after_seq {
        Some(value) if value < 0 => Err(SessionError::new(
            SessionErrorCode::InvalidQuery,
            "cursor sequence must be a non-negative integer",
        )),
        _ => Ok(()),
    }
}

/// Ordered iteration matching the TypeScript `ordered` helper.
fn ordered<T: Clone>(items: &[T], order: Option<EntryOrder>) -> Vec<T> {
    match order {
        Some(EntryOrder::OldestFirst) => items.to_vec(),
        _ => items.iter().rev().cloned().collect(),
    }
}

/// In-memory session state shared by both storage backends.
#[derive(Debug)]
pub struct SessionState {
    sequence: i64,
    used_ids: HashSet<String>,
    entries: Vec<Entry>,
    entries_by_id: HashMap<String, usize>,
    records: Vec<LaneRecord>,
    open_operations_by_lane: HashMap<String, Vec<LaneRecord>>,
    lanes: Vec<(String, Option<String>)>,
    log: Vec<LogItem>,
    stats: SessionStats,
    name: Option<String>,
    labels: HashMap<String, String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            sequence: 0,
            used_ids: HashSet::new(),
            entries: Vec::new(),
            entries_by_id: HashMap::new(),
            records: Vec::new(),
            open_operations_by_lane: HashMap::new(),
            lanes: vec![("main".to_owned(), None)],
            log: Vec::new(),
            stats: SessionStats::default(),
            name: None,
            labels: HashMap::new(),
        }
    }

    pub const fn next_sequence(&self) -> i64 {
        self.sequence + 1
    }

    pub fn get_lanes(&self) -> Vec<LanePointer> {
        self.lanes
            .iter()
            .map(|(lane, leaf_id)| LanePointer {
                lane: lane.clone(),
                leaf_id: leaf_id.clone(),
            })
            .collect()
    }

    pub fn require_lane(&self, lane: &str) -> Result<Option<String>, SessionError> {
        match self.lanes.iter().find(|(candidate, _)| candidate == lane) {
            Some((_, leaf_id)) => Ok(leaf_id.clone()),
            None => Err(SessionError::new(
                SessionErrorCode::InvalidLane,
                format!("Lane not found: {lane}"),
            )),
        }
    }

    pub fn validate_new_lane(&self, lane: &str) -> Result<(), SessionError> {
        if self.lanes.iter().any(|(candidate, _)| candidate == lane) {
            return Err(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Lane already exists: {lane}"),
            ));
        }
        Ok(())
    }

    pub fn validate_target(&self, target_id: Option<&str>) -> Result<(), SessionError> {
        match target_id {
            Some(target) if !self.entries_by_id.contains_key(target) => Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry not found: {target}"),
            )),
            _ => Ok(()),
        }
    }

    pub fn validate_unused_id(&self, id: &str) -> Result<(), SessionError> {
        if self.used_ids.contains(id) {
            return Err(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Session id already exists: {id}"),
            ));
        }
        Ok(())
    }

    pub fn apply_mutation(&mut self, mutation: &SessionMutation) -> Result<(), SessionError> {
        if mutation.seq() != self.sequence + 1 {
            return Err(invalid_mutation(format!("has non-consecutive seq {}", mutation.seq())));
        }

        match mutation {
            SessionMutation::Entry { lane, entry } => self.apply_entry(lane.as_deref(), entry),
            SessionMutation::Record { record } => self.apply_record(record),
            SessionMutation::Lane { seq, lane, leaf_id } => {
                if let Some(leaf) = leaf_id
                    && !self.entries_by_id.contains_key(leaf)
                {
                    return Err(invalid_mutation(format!("references missing lane target {leaf}")));
                }
                self.sequence = *seq;
                match self.lanes.iter_mut().find(|(candidate, _)| candidate == lane) {
                    Some((_, current)) => *current = leaf_id.clone(),
                    None => self.lanes.push((lane.clone(), leaf_id.clone())),
                }
                self.log.push(LogItem::Lane {
                    seq: *seq,
                    lane: lane.clone(),
                    leaf_id: leaf_id.clone(),
                });
                Ok(())
            }
            SessionMutation::Fact { seq, fact } => self.apply_fact(*seq, fact),
        }
    }

    fn apply_entry(&mut self, lane: Option<&str>, entry: &Entry) -> Result<(), SessionError> {
        self.validate_entry_for_lane(lane, entry)?;
        self.sequence = entry.seq;
        self.used_ids.insert(entry.id().to_owned());
        self.entries_by_id.insert(entry.id().to_owned(), self.entries.len());
        self.entries.push(entry.clone());
        if let Some(lane) = lane
            && let Some((_, leaf_id)) = self.lanes.iter_mut().find(|(candidate, _)| candidate == lane)
        {
            *leaf_id = Some(entry.id().to_owned());
        }
        self.log.push(LogItem::Entry {
            seq: entry.seq,
            entry: entry.clone(),
        });
        if entry.entry_type() == EntryType::Message {
            self.stats.message_count += 1;
        }
        Ok(())
    }

    fn validate_entry_for_lane(&self, lane: Option<&str>, entry: &Entry) -> Result<(), SessionError> {
        if self.used_ids.contains(entry.id()) {
            return Err(invalid_mutation(format!("contains duplicate id {}", entry.id())));
        }
        if let Some(lane) = lane {
            let leaf_id = match self.lanes.iter().find(|(candidate, _)| candidate == lane) {
                Some((_, leaf_id)) => leaf_id.clone(),
                None => return Err(invalid_mutation(format!("references missing lane {lane}"))),
            };
            if entry.parent_id != leaf_id {
                return Err(invalid_mutation("does not chain to the lane leaf"));
            }
        }
        if let Some(parent_id) = &entry.parent_id
            && !self.entries_by_id.contains_key(parent_id)
        {
            return Err(invalid_mutation(format!("references missing parent {parent_id}")));
        }
        Ok(())
    }

    fn apply_record(&mut self, record: &LaneRecord) -> Result<(), SessionError> {
        self.validate_record_identity(record)?;
        self.sequence = record.seq;
        self.used_ids.insert(record.id().to_owned());
        self.records.push(record.clone());
        self.track_open_operation(record);
        self.log.push(LogItem::Record {
            seq: record.seq,
            record: record.clone(),
        });
        self.accumulate_usage(record);
        Ok(())
    }

    fn validate_record_identity(&self, record: &LaneRecord) -> Result<(), SessionError> {
        if !self.lanes.iter().any(|(lane, _)| lane == record.lane()) {
            return Err(invalid_mutation(format!("references missing lane {}", record.lane())));
        }
        if self.used_ids.contains(record.id()) {
            return Err(invalid_mutation(format!("contains duplicate id {}", record.id())));
        }
        Ok(())
    }

    fn track_open_operation(&mut self, record: &LaneRecord) {
        match record.record_type() {
            RecordType::OperationStarted => {
                self.open_operations_by_lane
                    .entry(record.lane().to_owned())
                    .or_default()
                    .push(record.clone());
            }
            RecordType::OperationFinished => {
                if let Some(run_id) = record.body.run_id()
                    && let Some(open) = self.open_operations_by_lane.get_mut(record.lane())
                {
                    open.retain(|candidate| candidate.id() != run_id);
                }
            }
            _ => {}
        }
    }

    fn accumulate_usage(&mut self, record: &LaneRecord) {
        let Some(usage) = record.body.usage() else {
            return;
        };
        self.stats.cached_tokens += usage.cache_read;
        self.stats.uncached_tokens += usage.input + usage.cache_write;
        self.stats.total_tokens += usage.total_tokens;
        self.stats.cost_total += usage.cost.total;
    }

    fn apply_fact(&mut self, seq: i64, fact: &FactItem) -> Result<(), SessionError> {
        if let FactItem::Label { target_id, .. } = fact
            && !self.entries_by_id.contains_key(target_id)
        {
            return Err(invalid_mutation(format!("references missing label target {target_id}")));
        }
        self.sequence = seq;
        match fact {
            FactItem::Name { name } => self.name = name.clone(),
            FactItem::Label { target_id, label } => match label {
                Some(label) => {
                    self.labels.insert(target_id.clone(), label.clone());
                }
                None => {
                    self.labels.remove(target_id);
                }
            },
        }
        self.log.push(LogItem::Fact {
            seq,
            fact: fact.clone(),
        });
        Ok(())
    }

    pub fn get_entry(&self, id: &str) -> Option<&Entry> {
        self.entries_by_id.get(id).map(|index| &self.entries[*index])
    }

    pub fn find_entries(&self, query: &EntryQuery) -> Result<Vec<Entry>, SessionError> {
        assert_valid_limit(query.limit)?;
        assert_valid_cursor(query.cursor.map(|cursor| cursor.after_seq))?;
        let mut results = Vec::new();
        for entry in ordered(&self.entries, query.order) {
            if !matches_entry_query(&entry, query) {
                continue;
            }
            results.push(entry);
            if results.len() == query.limit.unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(results)
    }

    pub fn find_entries_on_branch(&self, query: &StorageBranchQuery) -> Result<Vec<Entry>, SessionError> {
        assert_valid_limit(query.entry.limit)?;
        assert_valid_cursor(query.entry.cursor.map(|cursor| cursor.after_seq))?;
        if query.entry.order == Some(EntryOrder::OldestFirst) {
            return self.find_branch_oldest_first(query);
        }
        self.find_branch_newest_first(query)
    }

    fn find_branch_oldest_first(&self, query: &StorageBranchQuery) -> Result<Vec<Entry>, SessionError> {
        let mut path: Vec<Entry> = self.walk_to_root(&query.start, None)?;
        path.reverse();
        let limit = query.entry.limit.unwrap_or(usize::MAX);
        let mut results = Vec::new();
        for entry in path {
            let reached_bound =
                Some(entry.id()) == query.stop_at_id.as_deref() || Some(entry.entry_type()) == query.stop_at_type;
            if matches_entry_query(&entry, &query.entry) {
                results.push(entry);
            }
            if reached_bound || results.len() == limit {
                break;
            }
        }
        Ok(results)
    }

    fn find_branch_newest_first(&self, query: &StorageBranchQuery) -> Result<Vec<Entry>, SessionError> {
        let bounds = BranchBounds {
            start: Some(query.start.clone()),
            stop_at_type: query.stop_at_type,
            stop_at_id: query.stop_at_id.clone(),
        };
        let limit = query.entry.limit.unwrap_or(usize::MAX);
        let mut results = Vec::new();
        for entry in self.walk_to_root(&query.start, Some(&bounds))? {
            if matches_entry_query(&entry, &query.entry) {
                results.push(entry);
            }
            if results.len() == limit {
                break;
            }
        }
        Ok(results)
    }

    pub fn find_records(&self, query: &RecordQuery) -> Result<Vec<LaneRecord>, SessionError> {
        assert_valid_limit(query.limit)?;
        assert_valid_cursor(query.after_seq)?;
        let mut results = Vec::new();
        for record in ordered(&self.records, query.order) {
            if !matches_record_query(&record, query) {
                continue;
            }
            results.push(record);
            if results.len() == query.limit.unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(results)
    }

    pub fn find_open_operations(&self, lane: &str, limit: Option<usize>) -> Result<Vec<LaneRecord>, SessionError> {
        assert_valid_limit(limit)?;
        let mut open: Vec<LaneRecord> = self
            .open_operations_by_lane
            .get(lane)
            .map(|records| records.iter().rev().cloned().collect())
            .unwrap_or_default();
        if let Some(limit) = limit {
            open.truncate(limit);
        }
        Ok(open)
    }

    pub fn get_log(&self, options: &LogOptions) -> Result<Vec<LogItem>, SessionError> {
        assert_valid_limit(options.limit)?;
        assert_valid_cursor(options.after_seq)?;
        let mut results = Vec::new();
        for item in &self.log {
            if options.after_seq.is_some_and(|after| item.seq() <= after) {
                continue;
            }
            results.push(item.clone());
            if results.len() == options.limit.unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(results)
    }

    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn get_label(&self, id: &str) -> Option<String> {
        self.labels.get(id).cloned()
    }

    pub const fn get_stats(&self) -> SessionStats {
        self.stats
    }

    pub fn create_fork_mutations(&self, options: &ForkOptions) -> Result<Vec<SessionMutation>, SessionError> {
        let (copied_entries, fork_lanes) = self.fork_sources(options)?;
        let mut mutations = fork_entry_mutations(&copied_entries);
        let mut sequence = mutations.len() as i64 + 1;
        for pointer in &fork_lanes {
            mutations.push(SessionMutation::Lane {
                seq: sequence,
                lane: pointer.lane.clone(),
                leaf_id: pointer.leaf_id.clone(),
            });
            sequence += 1;
        }
        if let Some(name) = &self.name {
            mutations.push(SessionMutation::Fact {
                seq: sequence,
                fact: FactItem::Name {
                    name: Some(name.clone()),
                },
            });
            sequence += 1;
        }
        mutations.extend(self.fork_label_mutations(&copied_entries, sequence));
        Ok(mutations)
    }

    fn fork_label_mutations(&self, copied_entries: &[Entry], mut sequence: i64) -> Vec<SessionMutation> {
        let mut mutations = Vec::new();
        for entry in copied_entries {
            let Some(label) = self.labels.get(entry.id()) else {
                continue;
            };
            mutations.push(SessionMutation::Fact {
                seq: sequence,
                fact: FactItem::Label {
                    target_id: entry.id().to_owned(),
                    label: Some(label.clone()),
                },
            });
            sequence += 1;
        }
        mutations
    }

    fn fork_sources(&self, options: &ForkOptions) -> Result<(Vec<Entry>, Vec<LanePointer>), SessionError> {
        if matches!(options, ForkOptions::Tree) {
            let entries = self.find_entries(&EntryQuery {
                order: Some(EntryOrder::OldestFirst),
                ..EntryQuery::default()
            })?;
            return Ok((entries, self.get_lanes()));
        }
        let target_id = self.fork_target(options)?;
        let entries = match &target_id {
            None => Vec::new(),
            Some(start) => self.find_entries_on_branch(&StorageBranchQuery {
                entry: EntryQuery {
                    order: Some(EntryOrder::OldestFirst),
                    ..EntryQuery::default()
                },
                start: start.clone(),
                stop_at_type: None,
                stop_at_id: None,
            })?,
        };
        let lanes = vec![LanePointer {
            lane: "main".to_owned(),
            leaf_id: target_id,
        }];
        Ok((entries, lanes))
    }

    fn fork_target(&self, options: &ForkOptions) -> Result<Option<String>, SessionError> {
        let selected_entry_id = match options {
            ForkOptions::Branch | ForkOptions::BranchDefaultBefore => self.require_lane("main")?,
            ForkOptions::BranchAt { entry_id } | ForkOptions::BranchBefore { entry_id } => Some(entry_id.clone()),
            ForkOptions::Tree => unreachable!("tree forks do not select a branch target"),
        };
        let Some(selected) = selected_entry_id else {
            return Ok(None);
        };
        let Some(entry) = self
            .get_entry(&selected)
            .filter(|entry| entry.entry_type() == EntryType::Message)
        else {
            return Err(SessionError::new(
                SessionErrorCode::InvalidForkTarget,
                format!("Fork target is not a message entry: {selected}"),
            ));
        };
        let before = matches!(
            options,
            ForkOptions::BranchBefore { .. } | ForkOptions::BranchDefaultBefore
        );
        Ok(if before {
            entry.parent_id.clone()
        } else {
            Some(entry.id().to_owned())
        })
    }

    fn walk_to_root(&self, start: &str, bounds: Option<&BranchBounds>) -> Result<Vec<Entry>, SessionError> {
        let mut visited = HashSet::new();
        let mut current = self.entry_or_not_found(start)?;
        let mut path = Vec::new();
        loop {
            if !visited.insert(current.id().to_owned()) {
                return Err(SessionError::new(
                    SessionErrorCode::InvalidEntry,
                    format!("Session branch contains a cycle at {}", current.id()),
                ));
            }
            let parent_id = current.parent_id.clone();
            let stop = reached_bound(&current, bounds) || parent_id.is_none();
            path.push(current);
            if stop {
                break;
            }
            current = self.entry_or_invalid(parent_id.expect("parent checked above"))?;
        }
        Ok(path)
    }

    fn entry_or_not_found(&self, id: &str) -> Result<Entry, SessionError> {
        match self.get_entry(id) {
            Some(entry) => Ok(entry.clone()),
            None => Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry not found: {id}"),
            )),
        }
    }

    fn entry_or_invalid(&self, id: String) -> Result<Entry, SessionError> {
        match self.get_entry(&id) {
            Some(entry) => Ok(entry.clone()),
            None => Err(SessionError::new(
                SessionErrorCode::InvalidEntry,
                format!("Entry not found: {id}"),
            )),
        }
    }
}

fn reached_bound(entry: &Entry, bounds: Option<&BranchBounds>) -> bool {
    let Some(bounds) = bounds else {
        return false;
    };
    Some(entry.id()) == bounds.stop_at_id.as_deref() || Some(entry.entry_type()) == bounds.stop_at_type
}

fn fork_entry_mutations(copied_entries: &[Entry]) -> Vec<SessionMutation> {
    copied_entries
        .iter()
        .enumerate()
        .map(|(index, source_entry)| SessionMutation::Entry {
            lane: None,
            entry: Entry {
                body: source_entry.body.clone(),
                parent_id: source_entry.parent_id.clone(),
                seq: index as i64 + 1,
                timestamp: source_entry.timestamp,
            },
        })
        .collect()
}

fn matches_entry_query(entry: &Entry, query: &EntryQuery) -> bool {
    let type_matches = query
        .entry_type
        .is_none_or(|entry_type| entry.entry_type() == entry_type);
    let custom_matches = query
        .custom_type
        .as_deref()
        .is_none_or(|custom_type| entry.body.custom_type() == Some(custom_type));
    let cursor_matches = match query.cursor {
        None => true,
        Some(cursor) if query.order == Some(EntryOrder::OldestFirst) => entry.seq > cursor.after_seq,
        Some(cursor) => entry.seq < cursor.after_seq,
    };
    type_matches && custom_matches && cursor_matches
}

fn matches_record_query(record: &LaneRecord, query: &RecordQuery) -> bool {
    let lane_matches = query.lane.as_deref().is_none_or(|lane| record.lane() == lane);
    let type_matches = query
        .record_type
        .is_none_or(|record_type| record.record_type() == record_type);
    let run_matches = query
        .run_id
        .as_deref()
        .is_none_or(|run_id| record.body.run_id() == Some(run_id));
    let kind_matches = query.operation_kind.is_none_or(|kind| match &record.body {
        crate::harness::session::types::RecordBody::OperationStarted { intent, .. } => intent.kind() == kind,
        _ => false,
    });
    let after_matches = query.after_seq.is_none_or(|after| record.seq > after);
    lane_matches && type_matches && run_matches && kind_matches && after_matches
}

impl OperationKind {
    pub const fn is_known(self) -> bool {
        matches!(self, Self::Run | Self::Compaction | Self::Navigation)
    }
}
