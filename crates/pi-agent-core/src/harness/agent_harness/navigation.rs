use super::super::compaction::{self, CompactOptions, DEFAULT_COMPACTION_SETTINGS, GenerateBranchSummaryOptions};
use super::super::messages::CustomMessageContent;
use super::super::session::{CompactionAppendOptions, KnownSessionEntry, MoveSummary};
use super::*;
use pi_ai::types::{TextOrImageContent, UserContent};
use pi_js_compat::js_format;

impl<M: SessionMetadataValue> AgentHarness<M> {
    fn begin_idle_operation(&self, phase: AgentHarnessPhase, message: &str) -> Result<(), AgentHarnessError> {
        let mut state = self.state();
        if state.phase != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, message));
        }
        state.phase = phase;
        Ok(())
    }
    pub async fn compact(&self, instructions: Option<JsString>) -> Result<HarnessCompactResult, AgentHarnessError> {
        self.begin_idle_operation(AgentHarnessPhase::Compaction, "compact() requires idle harness")?;
        let result = self.perform_compaction(instructions).await;
        self.state().phase = AgentHarnessPhase::Idle;
        result
    }
    async fn perform_compaction(
        &self,
        custom_instructions: Option<JsString>,
    ) -> Result<HarnessCompactResult, AgentHarnessError> {
        let model = self.get_model();
        let branch_entries = self.inner.session.get_branch(None).await?;
        let preparation = compaction::prepare_compaction(&branch_entries, &DEFAULT_COMPACTION_SETTINGS)?
            .ok_or_else(|| AgentHarnessError::new(AgentHarnessErrorCode::Compaction, "Nothing to compact"))?;
        let hook = self
            .hook(
                HarnessHookKind::SessionBeforeCompact,
                HarnessEvent::SessionBeforeCompact {
                    preparation: preparation.clone(),
                    branch_entries,
                    custom_instructions: custom_instructions.clone(),
                    signal: CancellationToken::new(),
                },
            )
            .await?;
        let provided = match hook {
            Some(HarnessHookResult::SessionBeforeCompact { cancel: true, .. }) => {
                return Err(AgentHarnessError::new(
                    AgentHarnessErrorCode::Compaction,
                    "Compaction cancelled",
                ));
            }
            Some(HarnessHookResult::SessionBeforeCompact { compaction, .. }) => compaction,
            _ => None,
        };
        let from_hook = provided.is_some();
        let result = if let Some(result) = provided {
            result
        } else {
            let result = compaction::compact(
                &preparation,
                self.inner.models.as_ref(),
                &model,
                &CompactOptions {
                    custom_instructions: custom_instructions.as_ref(),
                    thinking_level: Some(self.get_thinking_level()),
                    signal: None,
                },
            )
            .await?;
            HarnessCompactResult { summary: result.summary, first_kept_entry_id: result.first_kept_entry_id, tokens_before: result.tokens_before, duration_ms: result.duration_ms, details: result.details.map(|details| pi_js_compat::js_json!({"readFiles":details.read_files,"modifiedFiles":details.modified_files})) }
        };
        let id = self
            .inner
            .session
            .append_compaction(
                result.summary.clone(),
                result.first_kept_entry_id.clone(),
                result.tokens_before,
                CompactionAppendOptions {
                    details: result.details.clone(),
                    from_hook: Some(from_hook),
                    duration_ms: result.duration_ms,
                },
            )
            .await?;
        if let Some(entry) = self
            .inner
            .session
            .get_entry(&id)
            .await?
            .filter(|entry| entry.kind() == "compaction")
        {
            self.emit(
                HarnessEvent::SessionCompact {
                    compaction_entry: entry,
                    from_hook,
                },
                None,
            )
            .await?;
        }
        Ok(result)
    }
    pub async fn navigate_tree(
        &self,
        target_id: JsString,
        options: Option<NavigateTreeOptions>,
    ) -> Result<NavigateTreeResult, AgentHarnessError> {
        self.begin_idle_operation(AgentHarnessPhase::BranchSummary, "navigateTree() requires idle harness")?;
        let result = self.perform_navigation(target_id, options.unwrap_or_default()).await;
        self.state().phase = AgentHarnessPhase::Idle;
        result
    }
    async fn perform_navigation(
        &self,
        target_id: JsString,
        options: NavigateTreeOptions,
    ) -> Result<NavigateTreeResult, AgentHarnessError> {
        let old_leaf_id = self.inner.session.get_leaf_id().await?;
        if old_leaf_id.as_ref() == Some(&target_id) {
            return Ok(navigation_result(false));
        }
        let target = self.inner.session.get_entry(&target_id).await?.ok_or_else(|| {
            AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidArgument,
                js_format!("Entry {} not found", target_id),
            )
        })?;
        let collected =
            compaction::collect_entries_for_branch_summary(&self.inner.session, old_leaf_id.as_ref(), &target_id)
                .await?;
        let preparation = TreePreparation {
            target_id: target_id.clone(),
            old_leaf_id: old_leaf_id.clone(),
            common_ancestor_id: collected.common_ancestor_id,
            entries_to_summarize: collected.entries.clone(),
            user_wants_summary: options.summarize,
            custom_instructions: options.custom_instructions.clone(),
            replace_instructions: options.replace_instructions,
            label: options.label.clone(),
        };
        let hook = self
            .hook(
                HarnessHookKind::SessionBeforeTree,
                HarnessEvent::SessionBeforeTree {
                    preparation,
                    signal: CancellationToken::new(),
                },
            )
            .await?;
        let hook = match hook {
            Some(HarnessHookResult::SessionBeforeTree(result)) => result,
            _ => SessionBeforeTreeResult::default(),
        };
        if hook.cancel {
            return Ok(navigation_result(true));
        }
        let from_hook = hook.summary.is_some();
        let (mut summary, mut details) = hook
            .summary
            .map(|(text, details)| (Some(text), details))
            .unwrap_or_default();
        if summary.as_ref().is_none_or(JsString::is_empty) && options.summarize && !collected.entries.is_empty() {
            let model = self.get_model();
            let abort = CancellationToken::new();
            let instructions = hook.custom_instructions.or(options.custom_instructions);
            match compaction::generate_branch_summary(
                &collected.entries,
                &GenerateBranchSummaryOptions {
                    models: self.inner.models.as_ref(),
                    model: &model,
                    signal: Some(&abort),
                    custom_instructions: instructions.as_ref(),
                    replace_instructions: hook.replace_instructions.unwrap_or(options.replace_instructions),
                    reserve_tokens: None,
                },
            )
            .await
            {
                Ok(result) => {
                    summary = Some(result.summary);
                    details = Some(
                        pi_js_compat::js_json!({"readFiles":result.read_files,"modifiedFiles":result.modified_files}),
                    );
                }
                Err(error) if error.code == super::super::BranchSummaryErrorCode::Aborted => {
                    return Ok(navigation_result(true));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let (new_leaf, editor_text) = navigation_target(&target, &target_id);
        let summary = summary
            .filter(|summary| !summary.is_empty())
            .map(|summary| MoveSummary {
                summary,
                details,
                from_hook: Some(from_hook),
            });
        let id = self.inner.session.move_to(new_leaf.as_ref(), summary).await?;
        let summary_entry = match id {
            Some(id) => self
                .inner
                .session
                .get_entry(&id)
                .await?
                .filter(|entry| entry.kind() == "branch_summary"),
            None => None,
        };
        self.emit(
            HarnessEvent::SessionTree {
                new_leaf_id: self.inner.session.get_leaf_id().await?,
                old_leaf_id,
                summary_entry: summary_entry.clone(),
                from_hook,
            },
            None,
        )
        .await?;
        Ok(NavigateTreeResult {
            cancelled: false,
            editor_text,
            summary_entry,
        })
    }
}
fn navigation_result(cancelled: bool) -> NavigateTreeResult {
    NavigateTreeResult {
        cancelled,
        editor_text: None,
        summary_entry: None,
    }
}
fn navigation_target(
    entry: &super::super::session::SessionTreeEntry,
    target_id: &JsString,
) -> (Option<JsString>, Option<JsString>) {
    match entry.known() {
        Some(KnownSessionEntry::Message {
            base,
            message: crate::agent::AgentMessage::User(user),
        }) => {
            let text = match &user.content {
                UserContent::Text(text) => text.clone(),
                UserContent::Blocks(blocks) => text_blocks(blocks),
            };
            (base.parent_id.clone(), Some(text))
        }
        Some(KnownSessionEntry::CustomMessage { base, content, .. }) => {
            let text = match content {
                CustomMessageContent::Text(text) => text.clone(),
                CustomMessageContent::Blocks(blocks) => text_blocks(blocks),
            };
            (base.parent_id.clone(), Some(text))
        }
        _ => (Some(target_id.clone()), None),
    }
}
fn text_blocks(blocks: &[TextOrImageContent]) -> JsString {
    JsString::join(
        blocks.iter().filter_map(|block| match block {
            TextOrImageContent::Text(text) => Some(text.text.clone()),
            _ => None,
        }),
        "",
    )
}
