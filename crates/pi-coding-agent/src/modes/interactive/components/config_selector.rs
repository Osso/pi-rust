//! Ported from `coding-agent/src/modes/interactive/components/config-selector.ts`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pi_js_compat::collator::locale_compare;
use pi_tui::components::input::InputOptions;
use pi_tui::tui::component;
use pi_tui::utils::truncate_to_width_with;
use pi_tui::{Component, Container, Focusable, Input, Spacer, matches_key, visible_width};

use crate::config::package_config;
use crate::core::package_manager::{MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope};
use crate::core::settings_manager::{PackageFilter, PackageSource, SettingsManager};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};
use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::utils::paths::{PathInputOptions, canonicalize_path, is_local_path, relative_path, resolve_path};

/// Resource categories managed by the selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Extensions,
    Skills,
    Prompts,
    Themes,
}

impl ResourceType {
    const ALL: [Self; 4] = [Self::Extensions, Self::Skills, Self::Prompts, Self::Themes];

    fn as_str(self) -> &'static str {
        match self {
            Self::Extensions => "extensions",
            Self::Skills => "skills",
            Self::Prompts => "prompts",
            Self::Themes => "themes",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Extensions => "Extensions",
            Self::Skills => "Skills",
            Self::Prompts => "Prompts",
            Self::Themes => "Themes",
        }
    }

    fn order(self) -> usize {
        match self {
            Self::Extensions => 0,
            Self::Skills => 1,
            Self::Prompts => 2,
            Self::Themes => 3,
        }
    }
}

/// Settings file the selector writes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWriteScope {
    Global,
    Project,
}

/// Settings scope of one resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsScope {
    User,
    Project,
}

/// Project override state for one resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectOverrideState {
    Inherit,
    Load,
    Unload,
}

/// `ScopedResolvedPaths`.
#[derive(Clone)]
pub struct ScopedResolvedPaths {
    pub global: ResolvedPaths,
    pub project: ResolvedPaths,
}

#[derive(Clone)]
struct ResourceItem {
    path: String,
    enabled: bool,
    metadata: PathMetadata,
    resource_type: ResourceType,
    display_name: String,
}

#[derive(Clone)]
struct ResourceSubgroup {
    resource_type: ResourceType,
    label: String,
    items: Vec<ResourceItem>,
}

#[derive(Clone)]
struct ResourceGroup {
    label: String,
    scope: SourceScope,
    origin: MetadataOrigin,
    source: String,
    subgroups: Vec<ResourceSubgroup>,
}

#[derive(Debug, Clone, Copy)]
enum FlatEntry {
    Group(usize),
    Subgroup(usize, usize),
    Item(usize, usize, usize),
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn format_base_dir(base_dir: &str) -> String {
    let home_dir = home_dir();
    let display_path = if base_dir == home_dir {
        "~".to_string()
    } else if let Some(rest) = base_dir.strip_prefix(&home_dir) {
        format!("~{}", rest.replace('\\', "/"))
    } else {
        base_dir.replace('\\', "/")
    };
    if display_path.ends_with('/') {
        display_path
    } else {
        format!("{display_path}/")
    }
}

fn get_group_label(metadata: &PathMetadata, agent_dir: &str) -> String {
    if metadata.origin == MetadataOrigin::Package {
        return format!("{} ({})", metadata.source, scope_name(metadata.scope));
    }
    if metadata.source == "auto" {
        if let Some(base_dir) = &metadata.base_dir {
            return if metadata.scope == SourceScope::User {
                format!("User ({})", format_base_dir(base_dir))
            } else {
                format!("Project ({})", format_base_dir(base_dir))
            };
        }
        return if metadata.scope == SourceScope::User {
            format!("User ({})", format_base_dir(agent_dir))
        } else {
            format!("Project ({}/)", package_config().config_dir_name)
        };
    }
    if metadata.scope == SourceScope::User {
        "User settings".to_string()
    } else {
        "Project settings".to_string()
    }
}

fn scope_name(scope: SourceScope) -> &'static str {
    match scope {
        SourceScope::User => "user",
        SourceScope::Project => "project",
        SourceScope::Temporary => "temporary",
    }
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn parent_name(path: &str) -> String {
    Path::new(path)
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn build_groups(resolved: &ResolvedPaths, agent_dir: &str) -> Vec<ResourceGroup> {
    let mut groups: Vec<ResourceGroup> = Vec::new();
    let mut group_indices: Vec<(String, usize)> = Vec::new();

    let add_to_group = |resources: &[ResolvedResource],
                        resource_type: ResourceType,
                        groups: &mut Vec<ResourceGroup>,
                        group_indices: &mut Vec<(String, usize)>| {
        for resource in resources {
            let metadata = &resource.metadata;
            let group_key = format!(
                "{}:{}:{}:{}",
                origin_name(metadata.origin),
                scope_name(metadata.scope),
                metadata.source,
                metadata.base_dir.as_deref().unwrap_or("")
            );
            let group_index = match group_indices.iter().find(|(key, _)| *key == group_key) {
                Some((_, index)) => *index,
                None => {
                    groups.push(ResourceGroup {
                        label: get_group_label(metadata, agent_dir),
                        scope: metadata.scope,
                        origin: metadata.origin,
                        source: metadata.source.clone(),
                        subgroups: Vec::new(),
                    });
                    let index = groups.len() - 1;
                    group_indices.push((group_key.clone(), index));
                    index
                }
            };
            let group = &mut groups[group_index];
            let subgroup_index = match group
                .subgroups
                .iter()
                .position(|subgroup| subgroup.resource_type == resource_type)
            {
                Some(index) => index,
                None => {
                    group.subgroups.push(ResourceSubgroup {
                        resource_type,
                        label: resource_type.label().to_string(),
                        items: Vec::new(),
                    });
                    group.subgroups.len() - 1
                }
            };
            let name = file_name(&resource.path);
            let parent = parent_name(&resource.path);
            let display_name = if resource_type == ResourceType::Extensions && parent != "extensions" {
                format!("{parent}/{name}")
            } else if resource_type == ResourceType::Skills && name == "SKILL.md" {
                parent
            } else {
                name
            };
            group.subgroups[subgroup_index].items.push(ResourceItem {
                path: resource.path.clone(),
                enabled: resource.enabled,
                metadata: metadata.clone(),
                resource_type,
                display_name,
            });
        }
    };

    add_to_group(
        &resolved.extensions,
        ResourceType::Extensions,
        &mut groups,
        &mut group_indices,
    );
    add_to_group(&resolved.skills, ResourceType::Skills, &mut groups, &mut group_indices);
    add_to_group(
        &resolved.prompts,
        ResourceType::Prompts,
        &mut groups,
        &mut group_indices,
    );
    add_to_group(&resolved.themes, ResourceType::Themes, &mut groups, &mut group_indices);

    groups.sort_by(|a, b| {
        if a.origin != b.origin {
            return if a.origin == MetadataOrigin::Package {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if a.scope != b.scope {
            return if a.scope == SourceScope::User {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        locale_compare(&a.source, &b.source)
    });

    for group in &mut groups {
        group.subgroups.sort_by_key(|subgroup| subgroup.resource_type.order());
        for subgroup in &mut group.subgroups {
            subgroup
                .items
                .sort_by(|a, b| locale_compare(&a.display_name, &b.display_name));
        }
    }
    groups
}

fn origin_name(origin: MetadataOrigin) -> &'static str {
    match origin {
        MetadataOrigin::Package => "package",
        MetadataOrigin::TopLevel => "top-level",
    }
}

struct ConfigSelectorHeader {
    write_scope: ConfigWriteScope,
    project_mode_available: bool,
}

impl ConfigSelectorHeader {
    fn render_lines(&self, width: usize) -> Vec<String> {
        with_theme(|theme| {
            let title = theme.bold(if self.write_scope == ConfigWriteScope::Project {
                "Project Local Resources"
            } else {
                "Global Resources"
            });
            let separator = theme.fg(ThemeColor::Muted, " · ");
            let switch_hint = if self.project_mode_available {
                key_hint("tui.input.tab", "switch mode") + &separator
            } else {
                String::new()
            };
            let action_hint = if self.write_scope == ConfigWriteScope::Project {
                raw_key_hint("space", "cycle inherit/+/-")
            } else {
                raw_key_hint("space", "toggle")
            };
            let hint = switch_hint + &action_hint + &separator + &raw_key_hint("esc", "close");
            let spacing = width
                .saturating_sub(visible_width(&title) + visible_width(&hint))
                .max(1);
            let scope_hint = if self.write_scope == ConfigWriteScope::Project {
                theme.fg(
                    ThemeColor::Muted,
                    &format!(
                        "{}/settings.json · inherited global resources are dimmed",
                        package_config().config_dir_name
                    ),
                )
            } else {
                theme.fg(
                    ThemeColor::Muted,
                    &format!("~/{}/agent/settings.json", package_config().config_dir_name),
                )
            };
            vec![
                truncate_to_width_with(&format!("{title}{}{hint}", " ".repeat(spacing)), width, "", false),
                truncate_to_width_with(&scope_hint, width, "", false),
            ]
        })
    }
}

/// Callback fired after a resource toggle.
pub type ToggleCallback = Rc<dyn Fn(&str, bool)>;

/// Resource list with group headers, search, and enable/disable toggles.
pub struct ResourceList {
    groups_by_scope: [Vec<ResourceGroup>; 2],
    flat_items: Vec<FlatEntry>,
    filtered_items: Vec<FlatEntry>,
    selected_index: usize,
    search_input: Input,
    max_visible: usize,
    settings_manager: Rc<RefCell<SettingsManager>>,
    cwd: String,
    agent_dir: String,
    write_scope: ConfigWriteScope,
    inherited_enabled_by_key: Vec<(String, bool)>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_exit: Option<Rc<dyn Fn()>>,
    pub on_toggle: Option<ToggleCallback>,
    project_mode_available: bool,
    switch_requested: bool,
    focused: bool,
}

impl ResourceList {
    pub fn new(
        groups_by_scope: ScopedResolvedPaths,
        settings_manager: Rc<RefCell<SettingsManager>>,
        cwd: &str,
        agent_dir: &str,
        terminal_height: Option<usize>,
        write_scope: ConfigWriteScope,
    ) -> Self {
        let global = build_groups(&groups_by_scope.global, agent_dir);
        let project = build_groups(&groups_by_scope.project, agent_dir);
        let inherited_enabled_by_key = Self::build_inherited_enabled_map(&global);
        let chrome = 8;
        let max_visible = terminal_height.unwrap_or(24).saturating_sub(chrome).max(5);
        let mut list = Self {
            groups_by_scope: [global, project],
            flat_items: Vec::new(),
            filtered_items: Vec::new(),
            selected_index: 0,
            search_input: Input::new(InputOptions::default()),
            max_visible,
            settings_manager,
            cwd: cwd.to_string(),
            agent_dir: agent_dir.to_string(),
            write_scope,
            inherited_enabled_by_key,
            on_cancel: None,
            on_exit: None,
            on_toggle: None,
            project_mode_available: true,
            switch_requested: false,
            focused: false,
        };
        list.build_flat_list();
        list.filtered_items = list.flat_items.clone();
        list
    }

    fn scope_index(&self) -> usize {
        match self.write_scope {
            ConfigWriteScope::Global => 0,
            ConfigWriteScope::Project => 1,
        }
    }

    fn groups(&self) -> &[ResourceGroup] {
        &self.groups_by_scope[self.scope_index()]
    }

    fn groups_mut(&mut self) -> &mut Vec<ResourceGroup> {
        let index = self.scope_index();
        &mut self.groups_by_scope[index]
    }

    fn build_inherited_enabled_map(groups: &[ResourceGroup]) -> Vec<(String, bool)> {
        let mut result = Vec::new();
        for group in groups {
            for subgroup in &group.subgroups {
                for item in &subgroup.items {
                    result.push((Self::resource_item_key(item), item.enabled));
                }
            }
        }
        result
    }

    pub fn set_write_scope(&mut self, write_scope: ConfigWriteScope) {
        self.write_scope = write_scope;
        self.build_flat_list();
        let query = self.search_input.get_value().to_string();
        self.filter_items(&query);
    }

    fn build_flat_list(&mut self) {
        let mut flat_items = Vec::new();
        for (group_index, group) in self.groups().iter().enumerate() {
            flat_items.push(FlatEntry::Group(group_index));
            for (subgroup_index, subgroup) in group.subgroups.iter().enumerate() {
                flat_items.push(FlatEntry::Subgroup(group_index, subgroup_index));
                for item_index in 0..subgroup.items.len() {
                    flat_items.push(FlatEntry::Item(group_index, subgroup_index, item_index));
                }
            }
        }
        self.flat_items = flat_items;
        self.selected_index = self
            .flat_items
            .iter()
            .position(|entry| matches!(entry, FlatEntry::Item(_, _, _)))
            .unwrap_or(0);
    }

    fn find_next_item(&self, from_index: usize, direction: i64) -> usize {
        let mut index = from_index as i64 + direction;
        while index >= 0 && index < self.filtered_items.len() as i64 {
            if matches!(self.filtered_items[index as usize], FlatEntry::Item(_, _, _)) {
                return index as usize;
            }
            index += direction;
        }
        from_index
    }

    fn filter_items(&mut self, query: &str) {
        if query.trim().is_empty() {
            self.filtered_items = self.flat_items.clone();
            self.select_first_item();
            return;
        }
        let lower_query = query.to_lowercase();
        let groups = self.groups().to_vec();
        let mut matching_items: HashSet<(usize, usize, usize)> = HashSet::new();
        for (group_index, group) in groups.iter().enumerate() {
            for (subgroup_index, subgroup) in group.subgroups.iter().enumerate() {
                for (item_index, item) in subgroup.items.iter().enumerate() {
                    if item.display_name.to_lowercase().contains(&lower_query)
                        || item.resource_type.as_str().contains(&lower_query)
                        || item.path.to_lowercase().contains(&lower_query)
                    {
                        matching_items.insert((group_index, subgroup_index, item_index));
                    }
                }
            }
        }
        let mut matching_subgroups: HashSet<(usize, usize)> = HashSet::new();
        let mut matching_groups: HashSet<usize> = HashSet::new();
        for (group_index, group) in groups.iter().enumerate() {
            for (subgroup_index, subgroup) in group.subgroups.iter().enumerate() {
                for item_index in 0..subgroup.items.len() {
                    if matching_items.contains(&(group_index, subgroup_index, item_index)) {
                        matching_subgroups.insert((group_index, subgroup_index));
                        matching_groups.insert(group_index);
                    }
                }
            }
        }
        self.filtered_items = Vec::new();
        for entry in &self.flat_items {
            match *entry {
                FlatEntry::Group(index) if matching_groups.contains(&index) => {
                    self.filtered_items.push(*entry);
                }
                FlatEntry::Subgroup(group, subgroup) if matching_subgroups.contains(&(group, subgroup)) => {
                    self.filtered_items.push(*entry);
                }
                FlatEntry::Item(group, subgroup, item) if matching_items.contains(&(group, subgroup, item)) => {
                    self.filtered_items.push(*entry);
                }
                _ => {}
            }
        }
        self.select_first_item();
    }

    fn select_first_item(&mut self) {
        let first = self
            .filtered_items
            .iter()
            .position(|entry| matches!(entry, FlatEntry::Item(_, _, _)));
        self.selected_index = first.unwrap_or(0);
    }

    pub fn update_item(&mut self, path: &str, resource_type: ResourceType, enabled: bool) {
        for group in self.groups_mut() {
            for subgroup in &mut group.subgroups {
                if let Some(item) = subgroup
                    .items
                    .iter_mut()
                    .find(|item| item.path == path && item.resource_type == resource_type)
                {
                    item.enabled = enabled;
                    return;
                }
            }
        }
    }

    fn item_at(&self, entry: FlatEntry) -> Option<&ResourceItem> {
        match entry {
            FlatEntry::Item(group, subgroup, item) => Some(&self.groups()[group].subgroups[subgroup].items[item]),
            _ => None,
        }
    }

    fn selected_item(&self) -> Option<&ResourceItem> {
        let entry = *self.filtered_items.get(self.selected_index)?;
        self.item_at(entry)
    }

    fn render_checkbox(&self, item: &ResourceItem) -> String {
        with_theme(|theme| {
            if self.write_scope == ConfigWriteScope::Project {
                return match self.get_project_override_state(item) {
                    ProjectOverrideState::Load => theme.fg(ThemeColor::Success, "[+]"),
                    ProjectOverrideState::Unload => theme.fg(ThemeColor::Warning, "[-]"),
                    ProjectOverrideState::Inherit => {
                        if item.enabled {
                            theme.fg(ThemeColor::Dim, "[x]")
                        } else {
                            theme.fg(ThemeColor::Dim, "[ ]")
                        }
                    }
                };
            }
            if item.enabled {
                theme.fg(ThemeColor::Success, "[x]")
            } else {
                theme.fg(ThemeColor::Dim, "[ ]")
            }
        })
    }

    fn get_item_suffix(&self, item: &ResourceItem) -> String {
        if self.write_scope != ConfigWriteScope::Project {
            return String::new();
        }
        with_theme(|theme| match self.get_project_override_state(item) {
            ProjectOverrideState::Load => theme.fg(ThemeColor::Muted, "  project load"),
            ProjectOverrideState::Unload => theme.fg(ThemeColor::Muted, "  project unload"),
            ProjectOverrideState::Inherit => {
                if self.is_inherited_global_item(item) {
                    theme.fg(ThemeColor::Dim, "  inherited global")
                } else {
                    String::new()
                }
            }
        })
    }

    fn is_dimmed_item(&self, item: &ResourceItem) -> bool {
        self.write_scope == ConfigWriteScope::Project
            && self.is_inherited_global_item(item)
            && self.get_project_override_state(item) == ProjectOverrideState::Inherit
    }

    fn render_lines(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.search_input.render(width);
        lines.push(String::new());

        if self.filtered_items.is_empty() {
            lines.push(with_theme(|theme| theme.fg(ThemeColor::Muted, "  No resources found")));
            return lines;
        }

        let start_index = (self.selected_index as i64 - (self.max_visible / 2) as i64)
            .min(self.filtered_items.len() as i64 - self.max_visible as i64)
            .max(0) as usize;
        let end_index = (start_index + self.max_visible).min(self.filtered_items.len());

        for index in start_index..end_index {
            let entry = self.filtered_items[index];
            let is_selected = index == self.selected_index;
            match entry {
                FlatEntry::Group(group_index) => {
                    let group = &self.groups()[group_index];
                    let inherited = self.write_scope == ConfigWriteScope::Project && group.scope == SourceScope::User;
                    let label = with_theme(|theme| {
                        theme.bold(&format!(
                            "{}{}",
                            group.label,
                            if inherited { " · inherited global" } else { "" }
                        ))
                    });
                    let group_line = with_theme(|theme| {
                        theme.fg(if inherited { ThemeColor::Dim } else { ThemeColor::Accent }, &label)
                    });
                    lines.push(truncate_to_width_with(&format!("  {group_line}"), width, "", false));
                }
                FlatEntry::Subgroup(group_index, subgroup_index) => {
                    let group = &self.groups()[group_index];
                    let subgroup = &group.subgroups[subgroup_index];
                    let subgroup_line = with_theme(|theme| {
                        theme.fg(
                            if self.write_scope == ConfigWriteScope::Project && group.scope == SourceScope::User {
                                ThemeColor::Dim
                            } else {
                                ThemeColor::Muted
                            },
                            &subgroup.label,
                        )
                    });
                    lines.push(truncate_to_width_with(
                        &format!("    {subgroup_line}"),
                        width,
                        "",
                        false,
                    ));
                }
                FlatEntry::Item(group_index, subgroup_index, item_index) => {
                    let item = self.groups()[group_index].subgroups[subgroup_index].items[item_index].clone();
                    let cursor = if is_selected { "> " } else { "  " };
                    let dimmed = self.is_dimmed_item(&item);
                    let name_text = with_theme(|theme| {
                        if is_selected && !dimmed {
                            theme.bold(&item.display_name)
                        } else {
                            item.display_name.clone()
                        }
                    });
                    let name = if dimmed {
                        with_theme(|theme| theme.fg(ThemeColor::Dim, &name_text))
                    } else {
                        name_text
                    };
                    let checkbox = self.render_checkbox(&item);
                    let suffix = self.get_item_suffix(&item);
                    lines.push(truncate_to_width_with(
                        &format!("{cursor}    {checkbox} {name}{suffix}"),
                        width,
                        "...",
                        false,
                    ));
                }
            }
        }

        if start_index > 0 || end_index < self.filtered_items.len() {
            let item_count = self
                .filtered_items
                .iter()
                .filter(|entry| matches!(entry, FlatEntry::Item(_, _, _)))
                .count();
            let current_item_index = self.filtered_items[..self.selected_index]
                .iter()
                .filter(|entry| matches!(entry, FlatEntry::Item(_, _, _)))
                .count()
                + 1;
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Dim, &format!("  ({current_item_index}/{item_count})"))
            }));
        }
        lines
    }

    fn handle_key_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            self.selected_index = self.find_next_item(self.selected_index, -1);
            return;
        }
        if keybindings.matches(data, "tui.select.down") {
            self.selected_index = self.find_next_item(self.selected_index, 1);
            return;
        }
        if keybindings.matches(data, "tui.select.pageUp") {
            let mut target = self.selected_index.saturating_sub(self.max_visible);
            while target < self.filtered_items.len() && !matches!(self.filtered_items[target], FlatEntry::Item(_, _, _))
            {
                target += 1;
            }
            if target < self.filtered_items.len() {
                self.selected_index = target;
            }
            return;
        }
        if keybindings.matches(data, "tui.select.pageDown") {
            if self.filtered_items.is_empty() {
                return;
            }
            let mut target = (self.selected_index + self.max_visible).min(self.filtered_items.len().saturating_sub(1));
            while !matches!(self.filtered_items[target], FlatEntry::Item(_, _, _)) {
                if target == 0 {
                    break;
                }
                target -= 1;
            }
            if matches!(self.filtered_items[target], FlatEntry::Item(_, _, _)) {
                self.selected_index = target;
            }
            return;
        }
        if keybindings.matches(data, "tui.select.cancel") {
            if let Some(on_cancel) = &self.on_cancel {
                on_cancel();
            }
            return;
        }
        if matches_key(data, "ctrl+c") {
            if let Some(on_exit) = &self.on_exit {
                on_exit();
            }
            return;
        }
        if keybindings.matches(data, "tui.input.tab") {
            if self.project_mode_available {
                self.switch_requested = true;
            }
            return;
        }
        if data == " " || keybindings.matches(data, "tui.select.confirm") {
            let Some(item) = self.selected_item().cloned() else {
                return;
            };
            if self.write_scope != ConfigWriteScope::Project && self.get_item_scope(&item) != SettingsScope::User {
                return;
            }
            if let Some(new_enabled) = self.toggle_resource(&item) {
                self.update_item(&item.path, item.resource_type, new_enabled);
                if let Some(on_toggle) = &self.on_toggle {
                    on_toggle(&item.path, new_enabled);
                }
            }
            return;
        }
        self.search_input.handle_input(data);
        let query = self.search_input.get_value().to_string();
        self.filter_items(&query);
    }

    pub fn set_project_mode_available(&mut self, available: bool) {
        self.project_mode_available = available;
    }

    fn take_switch_request(&mut self) -> bool {
        std::mem::take(&mut self.switch_requested)
    }

    fn toggle_resource(&mut self, item: &ResourceItem) -> Option<bool> {
        if self.write_scope == ConfigWriteScope::Project {
            let state = self.get_next_override_state(item);
            if !self.set_project_resource_override(item, state) {
                return None;
            }
            return Some(match state {
                ProjectOverrideState::Inherit => self.get_inherited_enabled(item),
                ProjectOverrideState::Load => true,
                ProjectOverrideState::Unload => false,
            });
        }
        let enabled = !item.enabled;
        if item.metadata.origin == MetadataOrigin::TopLevel {
            self.toggle_top_level_resource(item, enabled);
        } else {
            self.toggle_package_resource(item, enabled);
        }
        Some(enabled)
    }

    fn toggle_top_level_resource(&mut self, item: &ResourceItem, enabled: bool) {
        let scope = item.metadata.scope;
        let array_key = item.resource_type.as_str();
        let current = self.settings_array(scope, array_key);
        let pattern = self.get_resource_pattern(item);
        let updated = Self::update_patterns(&current, &pattern, enabled);
        self.set_top_level_paths(scope, item.resource_type, &updated);
    }

    fn update_patterns(current: &[String], pattern: &str, enabled: bool) -> Vec<String> {
        let mut updated: Vec<String> = current
            .iter()
            .filter(|entry| {
                let stripped = entry
                    .strip_prefix('!')
                    .or_else(|| entry.strip_prefix('+'))
                    .or_else(|| entry.strip_prefix('-'))
                    .unwrap_or(entry);
                stripped != pattern
            })
            .cloned()
            .collect();
        updated.push(format!("{}{pattern}", if enabled { "+" } else { "-" }));
        updated
    }

    fn toggle_package_resource(&mut self, item: &ResourceItem, enabled: bool) {
        let scope = item.metadata.scope;
        let mut packages = self.get_packages(scope);
        let Some(package_index) = packages
            .iter()
            .position(|package| package.source() == item.metadata.source)
        else {
            return;
        };
        let pattern = self.get_package_resource_pattern(item);
        let package = &mut packages[package_index];
        let filter = Self::ensure_filter(package);
        let current = Self::filter_array(filter, item.resource_type);
        let updated = Self::update_patterns(&current, &pattern, enabled);
        Self::set_filter_array(filter, item.resource_type, updated);
        if ResourceType::ALL
            .iter()
            .all(|resource_type| Self::filter_array(filter, *resource_type).is_empty())
        {
            packages[package_index] = PackageSource::Name(filter.source.clone());
        }
        self.set_packages(scope, packages);
    }

    fn ensure_filter(package: &mut PackageSource) -> &mut PackageFilter {
        if matches!(package, PackageSource::Name(_)) {
            let source = package.source().to_string();
            *package = PackageSource::Filter(PackageFilter {
                source,
                ..Default::default()
            });
        }
        match package {
            PackageSource::Filter(filter) => filter,
            PackageSource::Name(_) => unreachable!("converted above"),
        }
    }

    fn filter_array(filter: &PackageFilter, resource_type: ResourceType) -> Vec<String> {
        Self::filter_array_ref(filter, resource_type)
            .cloned()
            .unwrap_or_default()
    }

    fn filter_array_ref(filter: &PackageFilter, resource_type: ResourceType) -> Option<&Vec<String>> {
        match resource_type {
            ResourceType::Extensions => filter.extensions.as_ref(),
            ResourceType::Skills => filter.skills.as_ref(),
            ResourceType::Prompts => filter.prompts.as_ref(),
            ResourceType::Themes => filter.themes.as_ref(),
        }
    }

    fn set_filter_array(filter: &mut PackageFilter, resource_type: ResourceType, values: Vec<String>) {
        let value = if values.is_empty() { None } else { Some(values) };
        match resource_type {
            ResourceType::Extensions => filter.extensions = value,
            ResourceType::Skills => filter.skills = value,
            ResourceType::Prompts => filter.prompts = value,
            ResourceType::Themes => filter.themes = value,
        }
    }

    fn settings_array(&self, scope: SourceScope, key: &str) -> Vec<String> {
        let settings = if scope == SourceScope::Project {
            self.settings_manager.borrow().get_project_settings()
        } else {
            self.settings_manager.borrow().get_global_settings()
        };
        settings
            .get(key)
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn set_top_level_paths(&self, scope: SourceScope, resource_type: ResourceType, paths: &[String]) {
        let mut settings = self.settings_manager.borrow_mut();
        let project = scope == SourceScope::Project;
        match (project, resource_type) {
            (false, ResourceType::Extensions) => settings.set_extension_paths(paths),
            (false, ResourceType::Skills) => settings.set_skill_paths(paths),
            (false, ResourceType::Prompts) => settings.set_prompt_template_paths(paths),
            (false, ResourceType::Themes) => settings.set_theme_paths(paths),
            (true, ResourceType::Extensions) => {
                let _ = settings.set_project_extension_paths(paths);
            }
            (true, ResourceType::Skills) => {
                let _ = settings.set_project_skill_paths(paths);
            }
            (true, ResourceType::Prompts) => {
                let _ = settings.set_project_prompt_template_paths(paths);
            }
            (true, ResourceType::Themes) => {
                let _ = settings.set_project_theme_paths(paths);
            }
        }
    }

    fn get_packages(&self, scope: SourceScope) -> Vec<PackageSource> {
        let settings = if scope == SourceScope::Project {
            self.settings_manager.borrow().get_project_settings()
        } else {
            self.settings_manager.borrow().get_global_settings()
        };
        settings
            .get("packages")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    fn set_packages(&self, scope: SourceScope, packages: Vec<PackageSource>) {
        let mut settings = self.settings_manager.borrow_mut();
        if scope == SourceScope::Project {
            let _ = settings.set_project_packages(packages);
        } else {
            settings.set_packages(packages);
        }
    }

    fn get_next_override_state(&self, item: &ResourceItem) -> ProjectOverrideState {
        let state = self.get_project_override_state(item);
        let inherited_enabled = self.get_inherited_enabled(item);
        match state {
            ProjectOverrideState::Inherit => {
                if inherited_enabled {
                    ProjectOverrideState::Unload
                } else {
                    ProjectOverrideState::Load
                }
            }
            ProjectOverrideState::Unload => {
                if inherited_enabled {
                    ProjectOverrideState::Load
                } else {
                    ProjectOverrideState::Inherit
                }
            }
            ProjectOverrideState::Load => {
                if inherited_enabled {
                    ProjectOverrideState::Inherit
                } else {
                    ProjectOverrideState::Unload
                }
            }
        }
    }

    fn get_project_override_state(&self, item: &ResourceItem) -> ProjectOverrideState {
        if self.write_scope != ConfigWriteScope::Project {
            return ProjectOverrideState::Inherit;
        }
        if item.metadata.origin == MetadataOrigin::TopLevel {
            return Self::get_override_state_from_entries(
                &self.settings_array(SourceScope::Project, item.resource_type.as_str()),
                &self.get_top_level_override_patterns(item, SettingsScope::Project),
                false,
            );
        }
        let Some(package) = self.find_matching_package_source(item, SettingsScope::Project) else {
            return ProjectOverrideState::Inherit;
        };
        let PackageSource::Filter(filter) = package else {
            return ProjectOverrideState::Inherit;
        };
        let Some(entries) = Self::filter_array_ref(&filter, item.resource_type) else {
            return ProjectOverrideState::Inherit;
        };
        let patterns: HashSet<String> = [self.get_package_resource_pattern(item)].into_iter().collect();
        Self::get_override_state_from_entries(entries, &patterns, filter.autoload != Some(false))
    }

    fn get_override_state_from_entries(
        entries: &[String],
        patterns: &HashSet<String>,
        empty_array_is_unload: bool,
    ) -> ProjectOverrideState {
        if entries.is_empty() && empty_array_is_unload {
            return ProjectOverrideState::Unload;
        }
        let mut state = ProjectOverrideState::Inherit;
        for entry in entries {
            if !patterns.contains(&Self::pattern_entry_target(entry)) {
                continue;
            }
            state = if entry.starts_with('!') || entry.starts_with('-') {
                ProjectOverrideState::Unload
            } else {
                ProjectOverrideState::Load
            };
        }
        state
    }

    fn get_inherited_enabled(&self, item: &ResourceItem) -> bool {
        let key = Self::resource_item_key(item);
        self.inherited_enabled_by_key
            .iter()
            .find(|(existing, _)| *existing == key)
            .map(|(_, enabled)| *enabled)
            .unwrap_or_else(|| {
                if self.get_item_scope(item) == SettingsScope::User {
                    item.enabled
                } else {
                    true
                }
            })
    }

    fn is_inherited_global_item(&self, item: &ResourceItem) -> bool {
        self.get_item_scope(item) == SettingsScope::User
            || self
                .inherited_enabled_by_key
                .iter()
                .any(|(key, _)| *key == Self::resource_item_key(item))
    }

    fn get_top_level_override_patterns(&self, item: &ResourceItem, scope: SettingsScope) -> HashSet<String> {
        let base_dir = self.get_top_level_base_dir(scope);
        let mut patterns: HashSet<String> = [
            self.get_resource_pattern_for_scope(item, scope),
            item.path.clone(),
            relative_path(&base_dir, &item.path),
        ]
        .into_iter()
        .collect();
        if let Some(metadata_base_dir) = &item.metadata.base_dir {
            patterns.insert(relative_path(metadata_base_dir, &item.path));
        }
        patterns
    }

    fn get_resource_pattern_for_scope(&self, item: &ResourceItem, scope: SettingsScope) -> String {
        let source_scope = self.get_item_scope(item);
        if scope != source_scope {
            return item.path.clone();
        }
        let base_dir = item
            .metadata
            .base_dir
            .clone()
            .unwrap_or_else(|| self.get_top_level_base_dir(source_scope));
        relative_path(&base_dir, &item.path)
    }

    fn set_project_resource_override(&mut self, item: &ResourceItem, state: ProjectOverrideState) -> bool {
        if item.metadata.origin == MetadataOrigin::TopLevel {
            self.set_project_top_level_override(item, state)
        } else {
            self.set_project_package_override(item, state)
        }
    }

    fn set_project_top_level_override(&mut self, item: &ResourceItem, state: ProjectOverrideState) -> bool {
        let current = self.settings_array(SourceScope::Project, item.resource_type.as_str());
        let pattern = if self.is_inherited_global_item(item) {
            item.path.clone()
        } else {
            self.get_resource_pattern_for_scope(item, SettingsScope::Project)
        };
        let patterns = self.get_top_level_override_patterns(item, SettingsScope::Project);
        let mut updated: Vec<String> = current
            .into_iter()
            .filter(|entry| {
                let target = Self::pattern_entry_target(entry);
                if (entry.starts_with('!') || entry.starts_with('+') || entry.starts_with('-'))
                    && patterns.contains(&target)
                {
                    return false;
                }
                !(state == ProjectOverrideState::Inherit && self.is_inherited_global_item(item) && target == pattern)
            })
            .collect();
        if state != ProjectOverrideState::Inherit {
            if self.is_inherited_global_item(item) && !updated.contains(&pattern) {
                updated.push(pattern.clone());
            }
            updated.push(format!(
                "{}{pattern}",
                if state == ProjectOverrideState::Load { "+" } else { "-" }
            ));
        }
        self.set_top_level_paths(SourceScope::Project, item.resource_type, &updated);
        true
    }

    fn set_project_package_override(&mut self, item: &ResourceItem, state: ProjectOverrideState) -> bool {
        let mut packages = self.get_packages(SourceScope::Project);
        let item_scope = settings_scope_name(self.get_item_scope(item));
        let mut package_index = packages.iter().position(|package| {
            self.package_sources_match(
                &item.metadata.source,
                item_scope,
                package.source(),
                settings_scope_name(SettingsScope::Project),
            )
        });
        if package_index.is_none() {
            if state == ProjectOverrideState::Inherit {
                return false;
            }
            packages.push(self.create_package_override_source(item));
            package_index = Some(packages.len() - 1);
        }
        let Some(package_index) = package_index else {
            return false;
        };
        let pattern = self.get_package_resource_pattern(item);
        let package = &mut packages[package_index];
        let filter = Self::ensure_filter(package);
        let updated: Vec<String> = Self::filter_array(filter, item.resource_type)
            .into_iter()
            .filter(|entry| Self::pattern_entry_target(entry) != pattern)
            .collect();
        let mut updated = updated;
        if state != ProjectOverrideState::Inherit {
            updated.push(format!(
                "{}{pattern}",
                if state == ProjectOverrideState::Load { "+" } else { "-" }
            ));
        }
        Self::set_filter_array(filter, item.resource_type, updated);
        if ResourceType::ALL
            .iter()
            .all(|resource_type| Self::filter_array(filter, *resource_type).is_empty())
        {
            if filter.autoload == Some(false) {
                packages.remove(package_index);
            } else {
                packages[package_index] = PackageSource::Name(filter.source.clone());
            }
        }
        self.set_packages(SourceScope::Project, packages);
        true
    }

    fn create_package_override_source(&self, item: &ResourceItem) -> PackageSource {
        let source = item.metadata.source.clone();
        if !is_local_path(&source) {
            return PackageSource::Filter(PackageFilter {
                source,
                autoload: Some(false),
                ..Default::default()
            });
        }
        let source_path = resolve_path(
            &source,
            &self.get_top_level_base_dir(self.get_item_scope(item)),
            &PathInputOptions {
                trim: true,
                ..Default::default()
            },
        )
        .unwrap_or(source);
        let project_base = self.get_top_level_base_dir(SettingsScope::Project);
        let relative = relative_path(&project_base, &source_path);
        PackageSource::Filter(PackageFilter {
            source: if relative.is_empty() { ".".to_string() } else { relative },
            autoload: Some(false),
            ..Default::default()
        })
    }

    fn find_matching_package_source(&self, item: &ResourceItem, target_scope: SettingsScope) -> Option<PackageSource> {
        let settings = if target_scope == SettingsScope::Project {
            self.settings_manager.borrow().get_project_settings()
        } else {
            self.settings_manager.borrow().get_global_settings()
        };
        let packages: Vec<PackageSource> = settings
            .get("packages")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        let target_scope_name = match target_scope {
            SettingsScope::Project => "project",
            SettingsScope::User => "user",
        };
        let item_scope = self.get_item_scope(item);
        let item_scope_name = match item_scope {
            SettingsScope::Project => "project",
            SettingsScope::User => "user",
        };
        packages.into_iter().find(|package| {
            self.package_sources_match(
                &item.metadata.source,
                item_scope_name,
                package.source(),
                target_scope_name,
            )
        })
    }

    fn package_sources_match(
        &self,
        left_source: &str,
        left_scope: &str,
        right_source: &str,
        right_scope: &str,
    ) -> bool {
        if left_source == right_source {
            return true;
        }
        if !is_local_path(left_source) || !is_local_path(right_source) {
            return false;
        }
        let options = PathInputOptions {
            trim: true,
            ..Default::default()
        };
        let left = resolve_path(left_source, &self.get_top_level_base_dir_str(left_scope), &options);
        let right = resolve_path(right_source, &self.get_top_level_base_dir_str(right_scope), &options);
        matches!((left, right), (Ok(left), Ok(right)) if left == right)
    }

    fn get_top_level_base_dir_str(&self, scope: &str) -> String {
        if scope == "project" {
            PathBuf::from(&self.cwd)
                .join(&package_config().config_dir_name)
                .to_string_lossy()
                .into_owned()
        } else {
            self.agent_dir.clone()
        }
    }

    fn pattern_entry_target(entry: &str) -> String {
        entry
            .strip_prefix('!')
            .or_else(|| entry.strip_prefix('+'))
            .or_else(|| entry.strip_prefix('-'))
            .unwrap_or(entry)
            .to_string()
    }

    fn resource_item_key(item: &ResourceItem) -> String {
        format!("{}:{}", item.resource_type.as_str(), canonicalize_path(&item.path))
    }

    fn get_item_scope(&self, item: &ResourceItem) -> SettingsScope {
        if item.metadata.scope == SourceScope::Project {
            SettingsScope::Project
        } else {
            SettingsScope::User
        }
    }

    fn get_top_level_base_dir(&self, scope: SettingsScope) -> String {
        match scope {
            SettingsScope::Project => PathBuf::from(&self.cwd)
                .join(&package_config().config_dir_name)
                .to_string_lossy()
                .into_owned(),
            SettingsScope::User => self.agent_dir.clone(),
        }
    }

    fn get_resource_pattern(&self, item: &ResourceItem) -> String {
        let base_dir = item
            .metadata
            .base_dir
            .clone()
            .unwrap_or_else(|| self.get_top_level_base_dir(scope_to_settings(item.metadata.scope)));
        relative_path(&base_dir, &item.path)
    }

    fn get_package_resource_pattern(&self, item: &ResourceItem) -> String {
        let base_dir = item.metadata.base_dir.clone().unwrap_or_else(|| {
            Path::new(&item.path)
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        relative_path(&base_dir, &item.path)
    }
}

fn settings_scope_name(scope: SettingsScope) -> &'static str {
    match scope {
        SettingsScope::Project => "project",
        SettingsScope::User => "user",
    }
}

fn scope_to_settings(scope: SourceScope) -> SettingsScope {
    if scope == SourceScope::Project {
        SettingsScope::Project
    } else {
        SettingsScope::User
    }
}

impl Focusable for ResourceList {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.set_focused(focused);
    }
}

impl Component for ResourceList {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.render_lines(width)
    }

    fn handle_input(&mut self, data: &str) {
        self.handle_key_input(data);
    }

    fn invalidate(&mut self) {
        self.search_input.invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

/// Config selector dialog for global/project resources.
pub struct ConfigSelectorComponent {
    header: ConfigSelectorHeader,
    resource_list: ResourceList,
    write_scope: ConfigWriteScope,
    request_render: Rc<dyn Fn()>,
    focused: bool,
    top: Container,
    bottom: Container,
}

/// Constructor inputs for `ConfigSelectorComponent`.
pub struct ConfigSelectorOptions {
    pub resolved_paths: ScopedResolvedPaths,
    pub settings_manager: Rc<RefCell<SettingsManager>>,
    pub cwd: String,
    pub agent_dir: String,
    pub on_close: Rc<dyn Fn()>,
    pub on_exit: Rc<dyn Fn()>,
    pub request_render: Rc<dyn Fn()>,
    pub terminal_height: Option<usize>,
    pub write_scope: ConfigWriteScope,
    pub project_mode_available: bool,
}

impl ConfigSelectorComponent {
    pub fn new(options: ConfigSelectorOptions) -> Self {
        let ConfigSelectorOptions {
            resolved_paths,
            settings_manager,
            cwd,
            agent_dir,
            on_close,
            on_exit,
            request_render,
            terminal_height,
            write_scope,
            project_mode_available,
        } = options;
        let mut resource_list = ResourceList::new(
            resolved_paths,
            settings_manager,
            &cwd,
            &agent_dir,
            terminal_height,
            write_scope,
        );
        resource_list.on_cancel = Some(on_close);
        resource_list.on_exit = Some(on_exit);
        resource_list.on_toggle = Some({
            let request_render = request_render.clone();
            Rc::new(move |_path: &str, _enabled: bool| request_render())
        });
        resource_list.set_project_mode_available(project_mode_available);

        let mut top = Container::new();
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(dynamic_border()));
        top.add_child(component(Spacer::new(1)));

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border()));

        Self {
            header: ConfigSelectorHeader {
                write_scope,
                project_mode_available,
            },
            resource_list,
            write_scope,
            request_render,
            focused: false,
            top,
            bottom,
        }
    }

    fn switch_write_scope(&mut self) {
        self.write_scope = if self.write_scope == ConfigWriteScope::Global {
            ConfigWriteScope::Project
        } else {
            ConfigWriteScope::Global
        };
        self.header.write_scope = self.write_scope;
        self.resource_list.set_write_scope(self.write_scope);
    }

    pub fn get_resource_list(&self) -> &ResourceList {
        &self.resource_list
    }

    pub fn get_resource_list_mut(&mut self) -> &mut ResourceList {
        &mut self.resource_list
    }
}

impl Focusable for ConfigSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.resource_list.set_focused(focused);
    }
}

impl Component for ConfigSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.header.render_lines(width));
        lines.push(String::new());
        lines.extend(self.resource_list.render(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        self.resource_list.handle_input(data);
        if self.resource_list.take_switch_request() {
            self.switch_write_scope();
            (self.request_render)();
        }
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.resource_list.invalidate();
        self.bottom.invalidate_children();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
