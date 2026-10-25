//! Port of `packages/ai/src/legacy-api-aliases.ts`.
//!
//! Deprecated per-API `stream`/`streamSimple` aliases. Each alias constructs the
//! same lazy api wrapper as the corresponding `*Api()` factory and forwards.

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::azure_openai_responses::azure_open_ai_responses_api;
use crate::api::google_generative_ai::google_generative_ai_api;
use crate::api::google_vertex::google_vertex_api;
use crate::api::mistral_conversations::mistral_conversations_api;
use crate::api::openai_codex_responses::openai_codex_responses_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::request::{ProviderStreams, SimpleStreamOptions, StreamOptions};
use crate::types::{Context, Model};
use crate::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::callback_error::CallbackError;

macro_rules! stream_aliases {
    ($stream:ident, $stream_simple:ident, $factory:path) => {
        /// Deprecated `stream` alias for the corresponding api module.
        pub fn $stream(
            model: Model,
            context: Context,
            options: StreamOptions,
        ) -> Result<AssistantMessageEventStream, CallbackError> {
            $factory().stream(model, context, options)
        }

        /// Deprecated `streamSimple` alias for the corresponding api module.
        pub fn $stream_simple(
            model: Model,
            context: Context,
            options: SimpleStreamOptions,
        ) -> Result<AssistantMessageEventStream, CallbackError> {
            $factory().stream_simple(model, context, options)
        }
    };
}

stream_aliases!(stream_anthropic, stream_simple_anthropic, anthropic_messages_api);
stream_aliases!(
    stream_azure_openai_responses,
    stream_simple_azure_openai_responses,
    azure_open_ai_responses_api
);
stream_aliases!(stream_google, stream_simple_google, google_generative_ai_api);
stream_aliases!(stream_google_vertex, stream_simple_google_vertex, google_vertex_api);
stream_aliases!(stream_mistral, stream_simple_mistral, mistral_conversations_api);
stream_aliases!(
    stream_openai_codex_responses,
    stream_simple_openai_codex_responses,
    openai_codex_responses_api
);
stream_aliases!(
    stream_openai_completions,
    stream_simple_openai_completions,
    open_ai_completions_api
);
stream_aliases!(
    stream_openai_responses,
    stream_simple_openai_responses,
    open_ai_responses_api
);
