//! Behavioral tests for `modes/interactive/components/oauth-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_ai::auth::types::{AuthCheck, AuthType};
use pi_coding_agent::modes::interactive::components::oauth_selector::{
    AuthSelectorMode, AuthSelectorProvider, OAuthSelectorComponent, format_auth_selector_provider_type,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::{Component, strip_terminal_sequences};

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

fn provider(id: &str, name: &str, auth_type: AuthType, status: Option<AuthCheck>) -> AuthSelectorProvider {
    AuthSelectorProvider {
        id: id.to_string(),
        name: name.to_string(),
        auth_type,
        method: None,
        status,
    }
}

fn rendered(selector: &mut OAuthSelectorComponent) -> String {
    selector
        .render(120)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn formats_provider_auth_types() {
    assert_eq!(format_auth_selector_provider_type(AuthType::Oauth), "subscription");
    assert_eq!(format_auth_selector_provider_type(AuthType::ApiKey), "API key");
}

#[test]
fn renders_an_option_without_compiled_auth_status_as_unconfigured() {
    let theme = install_theme();
    let mut selector = OAuthSelectorComponent::new(
        AuthSelectorMode::Login,
        vec![provider("google", "Google", AuthType::ApiKey, None)],
        Rc::new(|_id, _auth_type| {}),
        Rc::new(|| {}),
        None,
        theme,
    );
    let output = rendered(&mut selector);
    assert!(output.contains("unconfigured"), "{output}");
    assert!(!output.contains("✓ configured"), "{output}");
}

#[test]
fn shows_oauth_auth_distinctly_in_the_api_key_selector() {
    let theme = install_theme();
    let mut selector = OAuthSelectorComponent::new(
        AuthSelectorMode::Login,
        vec![provider(
            "anthropic",
            "Anthropic",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("OAuth".to_string()),
                auth_type: AuthType::Oauth,
            }),
        )],
        Rc::new(|_id, _auth_type| {}),
        Rc::new(|| {}),
        None,
        theme,
    );
    assert!(rendered(&mut selector).contains("subscription configured"));
}

#[test]
fn shows_environment_api_key_auth_as_configured() {
    let theme = install_theme();
    let mut selector = OAuthSelectorComponent::new(
        AuthSelectorMode::Login,
        vec![provider(
            "openai",
            "OpenAI",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("OPENAI_API_KEY".to_string()),
                auth_type: AuthType::ApiKey,
            }),
        )],
        Rc::new(|_id, _auth_type| {}),
        Rc::new(|| {}),
        None,
        theme,
    );
    let output = rendered(&mut selector);
    assert!(output.contains("✓ env: OPENAI_API_KEY"), "{output}");
    assert!(!output.contains("unconfigured"), "{output}");
}

#[test]
fn shows_models_json_api_key_auth_as_configured() {
    let theme = install_theme();
    let mut selector = OAuthSelectorComponent::new(
        AuthSelectorMode::Login,
        vec![provider(
            "local-proxy",
            "local-proxy",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("key in models.json".to_string()),
                auth_type: AuthType::ApiKey,
            }),
        )],
        Rc::new(|_id, _auth_type| {}),
        Rc::new(|| {}),
        None,
        theme,
    );
    assert!(rendered(&mut selector).contains("✓ key in models.json"));
}

#[test]
fn shows_models_json_command_auth_as_configured() {
    let theme = install_theme();
    let mut selector = OAuthSelectorComponent::new(
        AuthSelectorMode::Login,
        vec![provider(
            "op-proxy",
            "op-proxy",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("command in models.json".to_string()),
                auth_type: AuthType::ApiKey,
            }),
        )],
        Rc::new(|_id, _auth_type| {}),
        Rc::new(|| {}),
        None,
        theme,
    );
    assert!(rendered(&mut selector).contains("✓ command in models.json"));
}
