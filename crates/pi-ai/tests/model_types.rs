use pi_ai::types::{
    ImagesModel, Model, ModelCompat, OpenRouterRouting, PercentileThreshold, RoutingSort, VercelGatewayRouting,
};
use serde_json::json;

fn base_model(compat: serde_json::Value) -> serde_json::Value {
    json!({
        "id": "model", "name": "Model", "api": "openai-responses", "provider": "openai",
        "baseUrl": "https://example.invalid", "reasoning": true, "input": ["text"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 128000, "maxTokens": 16000, "compat": compat
    })
}

#[test]
fn openai_responses_compat_maps_to_the_types_module() {
    let model: Model = serde_json::from_value(base_model(json!({
        "supportsDeveloperRole": false,
        "sessionAffinityFormat": "openai-nosession",
        "supportsLongCacheRetention": true,
        "supportsToolSearch": true
    })))
    .expect("model parses");
    let Some(ModelCompat::OpenAIResponses(compat)) = model.compat else {
        panic!("expected responses compat")
    };
    assert_eq!(compat.supports_developer_role, Some(false));
    assert!(compat.supports_tool_search == Some(true));
}

#[test]
fn openrouter_routing_supports_string_and_object_sort_variants() {
    let strategy: OpenRouterRouting = serde_json::from_value(json!({
        "allow_fallbacks": false,
        "data_collection": "deny",
        "sort": "price",
        "max_price": {"prompt": 1, "completion": "0.5"},
        "preferred_min_throughput": 20,
        "preferred_max_latency": {"p90": 2.5}
    }))
    .expect("routing parses");
    assert_eq!(strategy.sort, Some(RoutingSort::Strategy("price".to_owned())));
    assert_eq!(strategy.data_collection, Some(pi_ai::types::DataCollection::Deny));
    assert_eq!(
        strategy.preferred_max_latency,
        Some(PercentileThreshold::Percentiles(pi_ai::types::PercentileCutoffs {
            p50: None,
            p75: None,
            p90: Some(2.5),
            p99: None,
        }))
    );

    let routing: OpenRouterRouting = serde_json::from_value(json!({
        "sort": {"by": "throughput", "partition": null},
        "preferred_min_throughput": 10
    }))
    .expect("routing parses");
    assert_eq!(
        routing.sort,
        Some(RoutingSort::Options(pi_ai::types::RoutingSortOptions {
            by: Some("throughput".to_owned()),
            partition: Some(None),
        }))
    );
    assert_eq!(
        routing.preferred_min_throughput,
        Some(PercentileThreshold::Number(10.0))
    );
}

#[test]
fn vercel_gateway_routing_maps_to_the_types_module() {
    let routing: VercelGatewayRouting = serde_json::from_value(json!({
        "only": ["bedrock"], "order": ["anthropic", "openai"]
    }))
    .expect("routing parses");
    assert_eq!(routing.only, Some(vec!["bedrock".to_owned()]));
    assert_eq!(routing.order, Some(vec!["anthropic".to_owned(), "openai".to_owned()]));
}

#[test]
fn images_model_maps_to_the_types_module() {
    let model: ImagesModel = serde_json::from_value(json!({
        "id": "image-model", "name": "Image Model", "api": "openrouter-images", "provider": "openrouter",
        "baseUrl": "https://example.invalid", "input": ["text"], "output": ["image"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
    }))
    .expect("images model parses");
    assert_eq!(model.output, vec![pi_ai::types::Modality::Image]);
}
