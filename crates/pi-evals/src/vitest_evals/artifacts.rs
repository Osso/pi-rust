//! Port of `packages/evals/src/vitest-evals/artifacts.ts`.
//!
//! Records native Pi session snapshots and eval source attachments against the
//! explicit test artifact list, and persists them under the artifact
//! directory for `runs.jsonl` references.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::PiEvalError;
use crate::harness::HarnessRun;

pub const PI_SESSION_SNAPSHOT_ARTIFACT: &str = "piSessionJsonl";

const SESSION_ARTIFACT_TYPE: &str = "@earendil-works/pi-evals:session";
const SOURCE_ARTIFACT_TYPE: &str = "@earendil-works/pi-evals:source";

/// `TestAttachment` as produced by the eval artifact helpers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAttachment {
    pub name: String,
    pub content_type: String,
    pub body: String,
    pub body_encoding: String,
}

impl TestAttachment {
    pub fn new(name: impl Into<String>, content_type: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content_type: content_type.into(),
            body: body.into(),
            body_encoding: "utf-8".to_owned(),
        }
    }
}

/// `TestArtifact` with the fields the reporter reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestArtifact {
    #[serde(rename = "type")]
    pub artifact_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<TestAttachment>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl TestArtifact {
    fn eval(artifact_type: &str, run_id: &str, attachments: Vec<TestAttachment>) -> Self {
        Self {
            artifact_type: artifact_type.to_owned(),
            run_id: Some(run_id.to_owned()),
            attachments: Some(attachments),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReference {
    pub name: String,
    pub path: String,
}

/// `recordEvalSessionArtifact(task, run)`.
pub fn record_eval_session_artifact(artifacts: &mut Vec<TestArtifact>, run: &HarnessRun) -> Result<(), PiEvalError> {
    let Some(run_artifacts) = run.artifacts.as_ref() else {
        return Ok(());
    };
    let Some(session_value) = run_artifacts.get(PI_SESSION_SNAPSHOT_ARTIFACT) else {
        return Ok(());
    };
    let run_id = run_artifacts.get("runId").and_then(Value::as_str);
    let session = session_value.as_str();
    let (Some(run_id), Some(session)) = (run_id, session) else {
        return Err(PiEvalError::type_error("Pi eval session artifact metadata is invalid."));
    };
    artifacts.push(TestArtifact::eval(
        SESSION_ARTIFACT_TYPE,
        run_id,
        vec![TestAttachment::new("session.jsonl", "application/jsonl", session)],
    ));
    Ok(())
}

/// `recordEvalSourceArtifact(task, runId, attachment)`.
pub fn record_eval_source_artifact(artifacts: &mut Vec<TestArtifact>, run_id: &str, attachment: TestAttachment) {
    artifacts.push(TestArtifact::eval(SOURCE_ARTIFACT_TYPE, run_id, vec![attachment]));
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn write_artifact_file(directory: &Path, name: &str, body: &str) -> Result<(), PiEvalError> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    let path = directory.join(name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    file.write_all(body.as_bytes())
        .map_err(|error| PiEvalError::error(error.to_string()))
}

/// `persistEvalArtifactReferences(artifacts, runId, artifactDirectory)`.
pub fn persist_eval_artifact_references(
    artifacts: &[TestArtifact],
    run_id: &str,
    artifact_directory: &str,
) -> Result<Vec<ArtifactReference>, PiEvalError> {
    let mut references = Vec::new();
    let run_hash = sha256_hex(run_id);
    for artifact in artifacts {
        if artifact.artifact_type != SESSION_ARTIFACT_TYPE && artifact.artifact_type != SOURCE_ARTIFACT_TYPE {
            continue;
        }
        if artifact.run_id.as_deref() != Some(run_id) {
            continue;
        }
        let category = if artifact.artifact_type == SESSION_ARTIFACT_TYPE {
            "sessions"
        } else {
            "sources"
        };
        for attachment in artifact.attachments.iter().flatten() {
            // `path.basename(name) !== name` for every name containing a path separator.
            if attachment.name.contains('/') {
                return Err(PiEvalError::type_error(format!(
                    "Invalid eval artifact name: {}",
                    attachment.name
                )));
            }
            let directory = Path::new(artifact_directory).join(category).join(&run_hash);
            write_artifact_file(&directory, &attachment.name, &attachment.body)?;
            references.push(ArtifactReference {
                name: attachment.name.clone(),
                path: format!("{category}/{run_hash}/{}", attachment.name),
            });
        }
    }
    Ok(references)
}
