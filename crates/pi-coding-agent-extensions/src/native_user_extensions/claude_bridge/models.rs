use pi_ai::types::{Model, ModelCost};
use pi_js_compat::js_json::JsValue;

const FAMILIES: [&str; 4] = ["fable", "opus", "sonnet", "haiku"];
const MEASURED_ONE_M: [&str; 9] = [
    "claude-fable-5",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-sonnet-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
];

fn version_rank(id: &str) -> (usize, u32, u32) {
    let parts: Vec<_> = id.split('-').collect();
    let family = parts
        .get(1)
        .and_then(|family| FAMILIES.iter().position(|item| item == family));
    let major = parts.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
    let minor = parts.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
    (family.unwrap_or(FAMILIES.len()), major, minor)
}

fn is_dated_alias(id: &str) -> bool {
    id.rsplit_once('-').is_some_and(|(_, suffix)| {
        suffix.len() == 8 && suffix.starts_with("20") && suffix.bytes().all(|c| c.is_ascii_digit())
    })
}

pub fn context_window(id: &str, settings: &JsValue) -> f64 {
    let forced = settings.get("forceTwoHundredK").and_then(JsValue::as_array);
    if forced.is_some_and(|ids| ids.iter().any(|v| v.as_string().is_some_and(|v| v == id))) {
        return 200_000.0;
    }
    let extra = settings.get("longContextExtraUsage").and_then(JsValue::as_bool) == Some(true);
    let max_plan = settings
        .get("plan")
        .and_then(JsValue::as_string)
        .is_some_and(|v| v == "max");
    let entitled = match id {
        "claude-opus-4-6" => max_plan || extra,
        "claude-sonnet-4-6" => extra,
        _ => MEASURED_ONE_M.contains(&id),
    };
    if entitled { 1_000_000.0 } else { 200_000.0 }
}

pub fn cli_model_id(id: &str, settings: &JsValue) -> String {
    if context_window(id, settings) > 200_000.0 {
        format!("{id}[1m]")
    } else {
        id.to_owned()
    }
}

pub fn project_models(models: Vec<Model>, settings: &JsValue) -> Vec<Model> {
    let mut models: Vec<_> = models
        .into_iter()
        .filter(|m| m.id != "claude-haiku-4-5" && !is_dated_alias(&m.id))
        .collect();
    models.sort_by(|a, b| {
        let (fa, ma, na) = version_rank(&a.id);
        let (fb, mb, nb) = version_rank(&b.id);
        fa.cmp(&fb).then(mb.cmp(&ma)).then(nb.cmp(&na)).then(a.id.cmp(&b.id))
    });
    for model in &mut models {
        model.context_window = context_window(&model.id, settings);
        let has_one_m_word = model.name.to_lowercase().split(' ').any(|part| part == "1m");
        if model.context_window > 200_000.0 && !has_one_m_word {
            model.name.push_str(" 1M");
        }
        model.cost = ModelCost::default();
    }
    models
}

pub fn resolve_model<'a>(models: &'a [Model], input: &str) -> Option<&'a Model> {
    let lower = input.to_lowercase();
    models.iter().find(|m| m.id == lower).or_else(|| {
        models.iter().filter(|m| m.id.contains(&lower)).max_by(|a, b| {
            let (_, ma, na) = version_rank(&a.id);
            let (_, mb, nb) = version_rank(&b.id);
            (ma, na).cmp(&(mb, nb)).then(std::cmp::Ordering::Greater)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;

    #[test]
    fn claude_bridge_catalog_has_user_models_measured_context_and_zero_cost() {
        let catalog = pi_ai::providers::models_data::provider_models("anthropic").unwrap();
        let models = project_models(catalog, &JsValue::Object(Default::default()));
        for id in ["claude-opus-5-5", "claude-haiku-5-5"] {
            let model = models.iter().find(|m| m.id == id).expect("user-enabled bridge model");
            assert_eq!(model.context_window, 1_000_000.0);
            assert!(model.name.ends_with("1M"));
            assert_eq!(model.cost.rates.input, 0.0);
            assert_eq!(model.cost.rates.output, 0.0);
        }
        assert!(!models.iter().any(|m| m.id == "claude-haiku-4-5"));
        assert!(!models.iter().any(|m| m.id.ends_with("-20251101")));
    }

    #[test]
    fn claude_bridge_plan_gates_and_force_200k_match_source() {
        let catalog = pi_ai::providers::models_data::provider_models("anthropic").unwrap();
        let settings = js_json::parse_lossless(r#"{"plan":"max","forceTwoHundredK":["claude-opus-5-5"]}"#).unwrap();
        let models = project_models(catalog, &settings);
        assert_eq!(
            models
                .iter()
                .find(|m| m.id == "claude-opus-4-6")
                .unwrap()
                .context_window,
            1_000_000.0
        );
        assert_eq!(
            models
                .iter()
                .find(|m| m.id == "claude-sonnet-4-6")
                .unwrap()
                .context_window,
            200_000.0
        );
        assert_eq!(
            models
                .iter()
                .find(|m| m.id == "claude-opus-5-5")
                .unwrap()
                .context_window,
            200_000.0
        );
    }
}
