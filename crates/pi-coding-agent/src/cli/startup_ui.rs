//! Startup UI helpers (`cli/startup-ui.ts`).
//!
//! Ported here: the official-distribution check, first-time-setup predicate,
//! theme loading, and startup theme resolution from settings.
//!
//! Blocked (documented in the traceability doc): `createStartupTui` /
//! `startStartupTui` / `applyDetectedStartupTheme` / `clearStartupTui` need a
//! `TerminalAutoThemeDetector` adapter over `TuiMainScreen`, and
//! `showStartupInput` / `showFirstTimeSetup` need `ExtensionInputComponent` and
//! `FirstTimeSetupComponent`.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::config;
use crate::core::package_manager::{
    DefaultPackageManager, MissingSourceAction, ResolvedResource, SettingsManagerStore,
};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use crate::modes::interactive::theme::{Theme, load_theme_from_path};

const OFFICIAL_PACKAGE_NAME: &str = "@earendil-works/pi-coding-agent";
const OFFICIAL_APP_NAME: &str = "pi";
const OFFICIAL_CONFIG_DIR_NAME: &str = ".pi";

/// `isOfficialDistribution`.
pub fn is_official_distribution(package_name: &str, app_name: &str, config_dir_name: &str) -> bool {
    package_name == OFFICIAL_PACKAGE_NAME
        && app_name == OFFICIAL_APP_NAME
        && config_dir_name == OFFICIAL_CONFIG_DIR_NAME
}

fn distribution_is_official() -> bool {
    let config = config::package_config();
    is_official_distribution(&config.package_name, &config.app_name, &config.config_dir_name)
}

/// `shouldRunFirstTimeSetup`, with the environment passed in so the predicate
/// can be tested without mutating the process environment.
pub fn should_run_first_time_setup(settings_path: &Path, env: &BTreeMap<String, String>) -> bool {
    if !distribution_is_official() {
        return false;
    }
    let experimental = env.get("PI_EXPERIMENTAL").is_some_and(|value| value == "1");
    if !experimental {
        return false;
    }
    if env.contains_key(&config::env_agent_dir()) {
        return false;
    }
    !settings_path.exists()
}

/// `shouldRunFirstTimeSetup()` with the process environment.
pub fn should_run_first_time_setup_for_process() -> bool {
    should_run_first_time_setup(&config::get_settings_path(), &std::env::vars().collect())
}

/// `loadThemes`: enabled resources, broken themes skipped, names deduped.
pub fn load_themes(resources: &[ResolvedResource]) -> Vec<Arc<Theme>> {
    let mut themes = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for resource in resources {
        if !resource.enabled {
            continue;
        }
        let Ok(loaded_theme) = load_theme_from_path(Path::new(&resource.path), None) else {
            continue;
        };
        if let Some(name) = &loaded_theme.name
            && !seen.insert(name.clone())
        {
            continue;
        }
        themes.push(Arc::new(loaded_theme));
    }
    themes
}

/// `loadStartupThemes`: global settings only, missing packages skipped.
pub async fn load_startup_themes(settings_manager: &SettingsManager) -> Result<Vec<Arc<Theme>>, String> {
    let global_settings_manager = SettingsManager::in_memory(
        settings_manager.get_global_settings(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_owned());
    let agent_dir = config::get_agent_dir().display().to_string();
    let package_manager = DefaultPackageManager::new(
        &cwd,
        &agent_dir,
        Arc::new(SettingsManagerStore::new(global_settings_manager)),
    );
    let resolved_paths = package_manager
        .resolve(Some(Box::new(|_source: String| {
            Box::pin(async { Ok(MissingSourceAction::Skip) })
        })))
        .await
        .map_err(|error| error.to_string())?;
    Ok(load_themes(&resolved_paths.themes))
}
