use pi_protocol::{ModelCost, ModelInput, ModelMetadata, ModelMetadataError, ModelMetadataInput, ThinkingLevel};

fn valid_input() -> ModelMetadataInput {
    ModelMetadataInput {
        provider: "openai".to_owned(),
        id: "gpt-5".to_owned(),
        name: "GPT-5".to_owned(),
        api: "openai-responses".to_owned(),
        reasoning: true,
        input: vec![ModelInput::Text],
        context_window: 128_000,
        max_tokens: 4_096,
        cost: ModelCost::new(1.0, 2.0, 0.0, -0.0).expect("valid model cost"),
        supported_thinking_levels: vec![ThinkingLevel::High],
        authenticated: true,
    }
}

#[test]
fn model_input_round_trips_exact_source_literals() {
    let cases = [("text", ModelInput::Text), ("image", ModelInput::Image)];

    for (literal, expected) in cases {
        let input = literal
            .parse::<ModelInput>()
            .expect("source model-input literals must parse");
        assert_eq!(input, expected);
        assert_eq!(input.as_str(), literal);
    }
}

#[test]
fn model_input_rejects_non_literals() {
    for value in ["", "TEXT", "image ", "audio", "é"] {
        assert!(
            value.parse::<ModelInput>().is_err(),
            "unexpectedly accepted model input {value:?}"
        );
    }
}

#[test]
fn preserves_every_metadata_field_and_array_order() {
    let cost = ModelCost::new(1_000.0, 110.5, 0.25, -0.0).expect("valid model cost");
    let input = ModelMetadataInput {
        provider: "provider".to_owned(),
        id: "model".to_owned(),
        name: "Model name".to_owned(),
        api: "provider-api".to_owned(),
        reasoning: false,
        input: vec![ModelInput::Image, ModelInput::Text, ModelInput::Text],
        context_window: 128_000,
        max_tokens: 4_096,
        cost,
        supported_thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Max, ThinkingLevel::Off],
        authenticated: false,
    };

    let metadata = ModelMetadata::new(input).expect("valid metadata must construct");

    assert_eq!(metadata.provider(), "provider");
    assert_eq!(metadata.id(), "model");
    assert_eq!(metadata.name(), "Model name");
    assert_eq!(metadata.api(), "provider-api");
    assert!(!metadata.reasoning());
    assert_eq!(
        metadata.input(),
        &[ModelInput::Image, ModelInput::Text, ModelInput::Text]
    );
    assert_eq!(metadata.context_window(), 128_000);
    assert_eq!(metadata.max_tokens(), 4_096);
    assert_eq!(metadata.cost().input(), 1_000.0);
    assert_eq!(metadata.cost().output(), 110.5);
    assert_eq!(metadata.cost().cache_read(), 0.25);
    assert_eq!(metadata.cost().cache_write().to_bits(), (-0.0f64).to_bits());
    assert_eq!(
        metadata.supported_thinking_levels(),
        &[ThinkingLevel::Off, ThinkingLevel::Max, ThinkingLevel::Off]
    );
    assert!(!metadata.authenticated());
}

#[test]
fn accepts_empty_input_unicode_strings_and_integer_boundaries() {
    let mut input = valid_input();
    input.provider = " \t".to_owned();
    input.id = "𐅑".to_owned();
    input.name = "e\u{301}".to_owned();
    input.api = "api\0name".to_owned();
    input.input.clear();
    input.context_window = 1;
    input.max_tokens = u64::MAX;

    let metadata = ModelMetadata::new(input).expect("representable schema values must construct");

    assert_eq!(metadata.provider(), " \t");
    assert_eq!(metadata.id(), "𐅑");
    assert_eq!(metadata.name(), "e\u{301}");
    assert_eq!(metadata.api(), "api\0name");
    assert!(metadata.input().is_empty());
    assert_eq!(metadata.context_window(), 1);
    assert_eq!(metadata.max_tokens(), u64::MAX);
}

#[test]
fn rejects_each_empty_string_in_source_order() {
    let mut input = valid_input();
    input.provider.clear();
    input.id.clear();
    input.name.clear();
    input.api.clear();
    assert_eq!(ModelMetadata::new(input), Err(ModelMetadataError::EmptyProvider));

    let mut input = valid_input();
    input.id.clear();
    assert_eq!(ModelMetadata::new(input), Err(ModelMetadataError::EmptyId));

    let mut input = valid_input();
    input.name.clear();
    assert_eq!(ModelMetadata::new(input), Err(ModelMetadataError::EmptyName));

    let mut input = valid_input();
    input.api.clear();
    assert_eq!(ModelMetadata::new(input), Err(ModelMetadataError::EmptyApi));
}

#[test]
fn rejects_zero_context_window_before_zero_max_tokens() {
    let mut input = valid_input();
    input.context_window = 0;
    input.max_tokens = 0;
    assert_eq!(ModelMetadata::new(input), Err(ModelMetadataError::ZeroContextWindow));

    let mut input = valid_input();
    input.max_tokens = 0;
    assert_eq!(ModelMetadata::new(input), Err(ModelMetadataError::ZeroMaxTokens));
}

#[test]
fn rejects_empty_supported_thinking_levels() {
    let mut input = valid_input();
    input.supported_thinking_levels.clear();

    assert_eq!(
        ModelMetadata::new(input),
        Err(ModelMetadataError::EmptySupportedThinkingLevels)
    );
}

#[test]
fn validation_errors_name_the_invalid_field() {
    assert_eq!(
        ModelMetadataError::ZeroMaxTokens.to_string(),
        "Model metadata field maxTokens must be greater than 0"
    );
}
