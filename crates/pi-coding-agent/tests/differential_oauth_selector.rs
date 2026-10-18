#[path = "support/oracle.rs"]
mod oracle;

use std::rc::Rc;
use std::sync::Arc;

use pi_ai::auth::types::{AuthCheck, AuthType};
use pi_coding_agent::modes::interactive::components::oauth_selector::{
    AuthSelectorMode, AuthSelectorProvider, OAuthSelectorComponent,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::Component;
use serde_json::{Value, json};

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

#[test]
fn oauth_selector_matches_the_pinned_typescript_source() {
    let theme = install_theme();
    pi_tui::set_keybindings(pi_tui::KeybindingsManager::new(
        pi_coding_agent::core::keybindings::keybindings(),
        Vec::new(),
    ));
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");

    let providers = vec![
        provider("anthropic", "Anthropic", AuthType::Oauth, None),
        provider(
            "openai",
            "OpenAI",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("OPENAI_API_KEY".to_string()),
                auth_type: AuthType::ApiKey,
            }),
        ),
        provider(
            "local-proxy",
            "local-proxy",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("key in models.json".to_string()),
                auth_type: AuthType::ApiKey,
            }),
        ),
        provider(
            "op-proxy",
            "op-proxy",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("command in models.json".to_string()),
                auth_type: AuthType::ApiKey,
            }),
        ),
        provider(
            "anthropic-key",
            "Anthropic",
            AuthType::ApiKey,
            Some(AuthCheck {
                source: Some("OAuth".to_string()),
                auth_type: AuthType::Oauth,
            }),
        ),
    ];
    let request_providers: Vec<Value> = providers
        .iter()
        .map(|provider| {
            json!({
                "id": provider.id,
                "name": provider.name,
                "authType": provider.auth_type.as_str(),
                "status": provider.status.as_ref().map(|status| json!({
                    "type": status.auth_type.as_str(),
                    "source": status.source,
                })),
            })
        })
        .collect();

    for (index, (mode, inputs)) in [
        (AuthSelectorMode::Login, vec!["\u{1b}[B", "\u{1b}[B", "\r"]),
        (AuthSelectorMode::Login, vec!["o", "p", "e", "n"]),
        (AuthSelectorMode::Logout, vec!["\u{1b}"]),
    ]
    .into_iter()
    .enumerate()
    {
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut selector = OAuthSelectorComponent::new(
            mode,
            providers.clone(),
            {
                let events = events.clone();
                Rc::new(move |id: &str, auth_type| {
                    events
                        .borrow_mut()
                        .push(json!({"select": id, "authType": auth_type.as_str()}));
                })
            },
            {
                let events = events.clone();
                Rc::new(move || events.borrow_mut().push(json!({"cancel": true})))
            },
            None,
            theme.clone(),
        );
        for input in &inputs {
            selector.handle_input(input);
        }
        let lines = selector.render(120);
        let expected = oracle.request(&json!({
            "id": 30 + index,
            "module": "modes/interactive/components/oauth-selector.ts",
            "operation": "oauthSelector",
            "mode": if mode == AuthSelectorMode::Login { "login" } else { "logout" },
            "providers": request_providers,
            "inputs": inputs,
            "width": 120,
        }));
        let actual = json!({"lines": lines, "events": events.borrow().clone()});
        assert_eq!(actual, expected["output"], "mismatch for case {index}");
    }
}
