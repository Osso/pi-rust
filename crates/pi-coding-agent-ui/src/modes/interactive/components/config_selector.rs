//! Ported from `coding-agent/src/modes/interactive/components/config-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use pi_js_compat::collator::locale_compare_utf16;
use pi_tui::components::input::InputOptions;
use pi_tui::tui::component;
use pi_tui::utils::truncate_to_width_with;
use pi_tui::{Component, Container, Focusable, Input, Spacer, matches_key, visible_width};

use crate::config::package_config;
use crate::core::package_manager::{MetadataOrigin, PathMetadata, ResolvedPaths, ResolvedResource, SourceScope};
use crate::core::settings_manager::{PackageFilter, PackageSource, SettingsManager};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::raw_key_hint;
use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::utils::paths::{node_basename, node_dirname, node_join, relative_path};

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

#[derive(Clone)]
struct ResourceItem {
    path: JsString,
    enabled: bool,
    metadata: PathMetadata,
    resource_type: ResourceType,
    display_name: JsString,
}

#[derive(Clone)]
struct ResourceSubgroup {
    resource_type: ResourceType,
    label: JsString,
    items: Vec<ResourceItem>,
}

#[derive(Clone)]
struct ResourceGroup {
    label: JsString,
    scope: SourceScope,
    origin: MetadataOrigin,
    source: JsString,
    subgroups: Vec<ResourceSubgroup>,
}

#[derive(Debug, Clone, Copy)]
enum FlatEntry {
    Group(usize),
    Subgroup(usize, usize),
    Item(usize, usize, usize),
}

fn format_base_dir(base_dir: &JsString) -> Result<JsString, pi_js_compat::node_error::NativeNodeError> {
    let home_dir = pi_js_compat::node_os::homedir()?;
    let display_path = if base_dir == &home_dir {
        JsString::from("~")
    } else if let Some(rest) = base_dir.strip_prefix(&home_dir) {
        js_format!("~{}", rest.replace('\\', "/"))
    } else {
        base_dir.replace('\\', "/")
    };
    Ok(if display_path.ends_with('/') {
        display_path
    } else {
        js_format!("{}/", display_path)
    })
}

fn get_group_label(
    metadata: &PathMetadata,
    agent_dir: &JsString,
) -> Result<JsString, pi_js_compat::node_error::NativeNodeError> {
    if metadata.origin == MetadataOrigin::Package {
        return Ok(js_format!("{} ({})", metadata.source, scope_name(metadata.scope)));
    }
    if metadata.source == "auto" {
        if let Some(base_dir) = &metadata.base_dir {
            return Ok(if metadata.scope == SourceScope::User {
                js_format!("User ({})", format_base_dir(base_dir)?)
            } else {
                js_format!("Project ({})", format_base_dir(base_dir)?)
            });
        }
        return Ok(if metadata.scope == SourceScope::User {
            js_format!("User ({})", format_base_dir(agent_dir)?)
        } else {
            js_format!("Project ({}/)", package_config().config_dir_name)
        });
    }
    Ok(if metadata.scope == SourceScope::User {
        JsString::from("User settings")
    } else {
        JsString::from("Project settings")
    })
}

fn scope_name(scope: SourceScope) -> &'static str {
    match scope {
        SourceScope::User => "user",
        SourceScope::Project => "project",
        SourceScope::Temporary => "temporary",
    }
}

fn file_name(path: &JsString) -> JsString {
    node_basename(path)
}
fn parent_name(path: &JsString) -> JsString {
    node_basename(node_dirname(path))
}

fn build_groups(
    resolved: &ResolvedPaths,
    agent_dir: &JsString,
) -> Result<Vec<ResourceGroup>, pi_js_compat::node_error::NativeNodeError> {
    let mut groups: Vec<ResourceGroup> = Vec::new();
    let mut group_indices: Vec<(JsString, usize)> = Vec::new();

    let add_to_group = |resources: &[ResolvedResource],
                        resource_type: ResourceType,
                        groups: &mut Vec<ResourceGroup>,
                        group_indices: &mut Vec<(JsString, usize)>|
     -> Result<(), pi_js_compat::node_error::NativeNodeError> {
        for resource in resources {
            let metadata = &resource.metadata;
            let group_key = js_format!(
                "{}:{}:{}:{}",
                origin_name(metadata.origin),
                scope_name(metadata.scope),
                metadata.source,
                metadata.base_dir.clone().unwrap_or_default()
            );
            let group_index = match group_indices.iter().find(|(key, _)| *key == group_key) {
                Some((_, index)) => *index,
                None => {
                    groups.push(ResourceGroup {
                        label: get_group_label(metadata, agent_dir)?,
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
                        label: resource_type.label().into(),
                        items: Vec::new(),
                    });
                    group.subgroups.len() - 1
                }
            };
            let name = file_name(&resource.path);
            let parent = parent_name(&resource.path);
            let display_name = if resource_type == ResourceType::Extensions && parent != "extensions" {
                js_format!("{}/{}", parent, name)
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
        Ok(())
    };

    add_to_group(
        &resolved.extensions,
        ResourceType::Extensions,
        &mut groups,
        &mut group_indices,
    )?;
    add_to_group(&resolved.skills, ResourceType::Skills, &mut groups, &mut group_indices)?;
    add_to_group(
        &resolved.prompts,
        ResourceType::Prompts,
        &mut groups,
        &mut group_indices,
    )?;
    add_to_group(&resolved.themes, ResourceType::Themes, &mut groups, &mut group_indices)?;

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
        locale_compare_utf16(a.source.units(), b.source.units())
    });

    for group in &mut groups {
        group.subgroups.sort_by_key(|subgroup| subgroup.resource_type.order());
        for subgroup in &mut group.subgroups {
            subgroup
                .items
                .sort_by(|a, b| locale_compare_utf16(a.display_name.units(), b.display_name.units()));
        }
    }
    Ok(groups)
}

fn origin_name(origin: MetadataOrigin) -> &'static str {
    match origin {
        MetadataOrigin::Package => "package",
        MetadataOrigin::TopLevel => "top-level",
    }
}

struct ConfigSelectorHeader;

impl ConfigSelectorHeader {
    fn render_lines(&self, width: usize) -> Vec<JsString> {
        with_theme(|theme| {
            let title = theme.bold("Resource Configuration");
            let separator = theme.fg(ThemeColor::Muted, " · ");
            let hint = raw_key_hint("space", "toggle") + &separator + &raw_key_hint("esc", "close");
            let spacing = width
                .saturating_sub(visible_width(&title) + visible_width(&hint))
                .max(1);
            vec![
                truncate_to_width_with(
                    &js_format!("{}{}{}", title, " ".repeat(spacing), hint),
                    width,
                    "",
                    false,
                ),
                theme.fg(ThemeColor::Muted, "Type to filter resources"),
            ]
        })
    }
}

/// Callback fired after a resource toggle.
pub type ToggleCallback = Rc<dyn Fn(&JsString, bool)>;

/// Resource list with group headers, search, and enable/disable toggles.
pub struct ResourceList {
    groups: Vec<ResourceGroup>,
    flat_items: Vec<FlatEntry>,
    filtered_items: Vec<FlatEntry>,
    selected_index: usize,
    search_input: Input,
    max_visible: usize,
    settings_manager: Rc<RefCell<SettingsManager>>,
    cwd: JsString,
    agent_dir: JsString,

    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_exit: Option<Rc<dyn Fn()>>,
    pub on_toggle: Option<ToggleCallback>,

    focused: bool,
}

impl ResourceList {
    pub fn new(
        resolved_paths: ResolvedPaths,
        settings_manager: Rc<RefCell<SettingsManager>>,
        cwd: &JsString,
        agent_dir: &JsString,
        terminal_height: Option<usize>,
    ) -> Result<Self, pi_js_compat::node_error::NativeNodeError> {
        let groups = build_groups(&resolved_paths, agent_dir)?;
        let chrome = 8;
        let max_visible = terminal_height.unwrap_or(24).saturating_sub(chrome).max(5);
        let mut list = Self {
            groups,
            flat_items: Vec::new(),
            filtered_items: Vec::new(),
            selected_index: 0,
            search_input: Input::new(InputOptions::default()),
            max_visible,
            settings_manager,
            cwd: cwd.clone(),
            agent_dir: agent_dir.clone(),

            on_cancel: None,
            on_exit: None,
            on_toggle: None,

            focused: false,
        };
        list.build_flat_list();
        list.filtered_items = list.flat_items.clone();
        Ok(list)
    }

    fn groups(&self) -> &[ResourceGroup] {
        &self.groups
    }

    fn groups_mut(&mut self) -> &mut Vec<ResourceGroup> {
        &mut self.groups
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

    fn filter_items(&mut self, query: &JsString) {
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
                        || JsString::from(item.resource_type.as_str()).contains(&lower_query)
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

    pub fn update_item(&mut self, path: &JsString, resource_type: ResourceType, enabled: bool) {
        for group in self.groups_mut() {
            for subgroup in &mut group.subgroups {
                if let Some(item) = subgroup
                    .items
                    .iter_mut()
                    .find(|item| &item.path == path && item.resource_type == resource_type)
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

    fn render_lines(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.search_input.render(width);
        lines.push(JsString::default());

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
                    let group_line = with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold(&group.label)));
                    lines.push(truncate_to_width_with(
                        &js_format!("  {}", group_line),
                        width,
                        "",
                        false,
                    ));
                }
                FlatEntry::Subgroup(group_index, subgroup_index) => {
                    let group = &self.groups()[group_index];
                    let subgroup = &group.subgroups[subgroup_index];
                    let subgroup_line = with_theme(|theme| theme.fg(ThemeColor::Muted, &subgroup.label));
                    lines.push(truncate_to_width_with(
                        &js_format!("    {}", subgroup_line),
                        width,
                        "",
                        false,
                    ));
                }
                FlatEntry::Item(group_index, subgroup_index, item_index) => {
                    let item = self.groups()[group_index].subgroups[subgroup_index].items[item_index].clone();
                    let cursor = if is_selected { "> " } else { "  " };
                    let name = with_theme(|theme| {
                        if is_selected {
                            theme.bold(&item.display_name)
                        } else {
                            item.display_name.clone()
                        }
                    });
                    let checkbox = with_theme(|theme| {
                        if item.enabled {
                            theme.fg(ThemeColor::Success, "[x]")
                        } else {
                            theme.fg(ThemeColor::Dim, "[ ]")
                        }
                    });
                    lines.push(truncate_to_width_with(
                        &js_format!("{}    {} {}", cursor, checkbox, name),
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
                theme.fg(
                    ThemeColor::Dim,
                    &js_format!("  ({}/{})", current_item_index, item_count),
                )
            }));
        }
        lines
    }

    fn handle_key_input(&mut self, data: &JsString) {
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
        if data == " " || keybindings.matches(data, "tui.select.confirm") {
            let Some(item) = self.selected_item().cloned() else {
                return;
            };
            let new_enabled = !item.enabled;
            self.toggle_resource(&item, new_enabled);
            self.update_item(&item.path, item.resource_type, new_enabled);
            if let Some(on_toggle) = &self.on_toggle {
                on_toggle(&item.path, new_enabled);
            }
            return;
        }
        self.search_input.handle_input(data);
        let query = self.search_input.get_value().clone();
        self.filter_items(&query);
    }

    fn toggle_resource(&mut self, item: &ResourceItem, enabled: bool) {
        if item.metadata.origin == MetadataOrigin::TopLevel {
            self.toggle_top_level_resource(item, enabled);
        } else {
            self.toggle_package_resource(item, enabled);
        }
    }

    fn toggle_top_level_resource(&mut self, item: &ResourceItem, enabled: bool) {
        let scope = item.metadata.scope;
        let array_key = item.resource_type.as_str();
        let current = self.settings_array(scope, array_key);
        let pattern = self.get_resource_pattern(item);
        let updated = Self::update_patterns(&current, &pattern, enabled);
        self.set_top_level_paths(scope, item.resource_type, &updated);
    }

    fn update_patterns(current: &[JsString], pattern: &JsString, enabled: bool) -> Vec<JsString> {
        let mut updated: Vec<JsString> = current
            .iter()
            .filter(|entry| {
                let stripped = entry
                    .strip_prefix('!')
                    .or_else(|| entry.strip_prefix('+'))
                    .or_else(|| entry.strip_prefix('-'))
                    .unwrap_or_else(|| (*entry).clone());
                stripped != *pattern
            })
            .cloned()
            .collect();
        updated.push(js_format!("{}{}", if enabled { "+" } else { "-" }, pattern));
        updated
    }

    fn toggle_package_resource(&mut self, item: &ResourceItem, enabled: bool) {
        let scope = item.metadata.scope;
        let mut packages = self.get_packages(scope);
        let Some(package_index) = packages
            .iter()
            .position(|package| package.source() == &item.metadata.source)
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
            .all(|resource_type| Self::filter_array_ref(filter, *resource_type).is_none())
        {
            packages[package_index] = PackageSource::Name(filter.source.clone());
        }
        self.set_packages(scope, packages);
    }

    fn ensure_filter(package: &mut PackageSource) -> &mut PackageFilter {
        if matches!(package, PackageSource::Name(_)) {
            let source = package.source().clone();
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

    fn filter_array(filter: &PackageFilter, resource_type: ResourceType) -> Vec<JsString> {
        Self::filter_array_ref(filter, resource_type)
            .cloned()
            .unwrap_or_default()
    }

    fn filter_array_ref(filter: &PackageFilter, resource_type: ResourceType) -> Option<&Vec<JsString>> {
        match resource_type {
            ResourceType::Extensions => filter.extensions.as_ref(),
            ResourceType::Skills => filter.skills.as_ref(),
            ResourceType::Prompts => filter.prompts.as_ref(),
            ResourceType::Themes => filter.themes.as_ref(),
        }
    }

    fn set_filter_array(filter: &mut PackageFilter, resource_type: ResourceType, values: Vec<JsString>) {
        let value = if values.is_empty() { None } else { Some(values) };
        match resource_type {
            ResourceType::Extensions => filter.extensions = value,
            ResourceType::Skills => filter.skills = value,
            ResourceType::Prompts => filter.prompts = value,
            ResourceType::Themes => filter.themes = value,
        }
    }

    fn settings_array(&self, scope: SourceScope, key: &str) -> Vec<JsString> {
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
                    .map(|value| {
                        value.as_string().cloned().unwrap_or_else(|| {
                            crate::utils::uncaught::throw_uncaught("TypeError", "Expected resource path string")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn set_top_level_paths(&self, scope: SourceScope, resource_type: ResourceType, paths: &[JsString]) {
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
            .map(|value| {
                pi_js_compat::js_json::from_value(value)
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught("Error", error.to_string()))
            })
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

    fn get_top_level_base_dir(&self, scope: SourceScope) -> JsString {
        match scope {
            SourceScope::Project => node_join(&self.cwd, &package_config().config_dir_name),
            _ => self.agent_dir.clone(),
        }
    }

    fn get_resource_pattern(&self, item: &ResourceItem) -> JsString {
        let base_dir = item
            .metadata
            .base_dir
            .clone()
            .unwrap_or_else(|| self.get_top_level_base_dir(item.metadata.scope));
        relative_path(&base_dir, &item.path)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught("Error", error.message().clone()))
    }

    fn get_package_resource_pattern(&self, item: &ResourceItem) -> JsString {
        let base_dir = item
            .metadata
            .base_dir
            .clone()
            .unwrap_or_else(|| node_dirname(&item.path));
        relative_path(&base_dir, &item.path)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught("Error", error.message().clone()))
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
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.render_lines(width)
    }

    fn handle_input(&mut self, data: &JsString) {
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

    focused: bool,
    top: Container,
    bottom: Container,
}

/// Constructor inputs for `ConfigSelectorComponent`.
pub struct ConfigSelectorOptions {
    pub resolved_paths: ResolvedPaths,
    pub settings_manager: Rc<RefCell<SettingsManager>>,
    pub cwd: pi_js_compat::js_json::JsString,
    pub agent_dir: pi_js_compat::js_json::JsString,
    pub on_close: Rc<dyn Fn()>,
    pub on_exit: Rc<dyn Fn()>,
    pub request_render: Rc<dyn Fn()>,
    pub terminal_height: Option<usize>,
}

impl ConfigSelectorComponent {
    pub fn new(options: ConfigSelectorOptions) -> Result<Self, pi_js_compat::node_error::NativeNodeError> {
        let ConfigSelectorOptions {
            resolved_paths,
            settings_manager,
            cwd,
            agent_dir,
            on_close,
            on_exit,
            request_render,
            terminal_height,
        } = options;
        let mut resource_list = ResourceList::new(resolved_paths, settings_manager, &cwd, &agent_dir, terminal_height)?;
        resource_list.on_cancel = Some(on_close);
        resource_list.on_exit = Some(on_exit);
        resource_list.on_toggle = Some({
            let request_render = request_render.clone();
            Rc::new(move |_path: &JsString, _enabled: bool| request_render())
        });

        let mut top = Container::new();
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(dynamic_border()));
        top.add_child(component(Spacer::new(1)));

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border()));

        Ok(Self {
            header: ConfigSelectorHeader,
            resource_list,

            focused: false,
            top,
            bottom,
        })
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
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.header.render_lines(width));
        lines.push(JsString::default());
        lines.extend(self.resource_list.render(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &JsString) {
        self.resource_list.handle_input(data);
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
