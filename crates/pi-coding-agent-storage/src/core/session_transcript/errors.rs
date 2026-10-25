//! Preserve libzstd's numeric failures across the required std::io::Read trait.

use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::NativeNodeError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub(super) enum ZstdReadFailure {
    Library(usize),
    Incomplete,
}

impl fmt::Display for ZstdReadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(code) => formatter.write_str(zstd::zstd_safe::get_error_name(*code)),
            Self::Incomplete => formatter.write_str("unexpected end of file"),
        }
    }
}
impl std::error::Error for ZstdReadFailure {}

impl ZstdReadFailure {
    pub(super) fn into_io(self) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, self)
    }

    pub(super) fn native_error(&self) -> NativeNodeError {
        let (errno, code) = match self {
            Self::Incomplete => (-5, "Z_BUF_ERROR"),
            Self::Library(encoded) => {
                let number = encoded.wrapping_neg();
                let name = ZSTD_ERROR_CODES.iter().find(|(code, _)| *code == number);
                let Some((_, name)) = name else {
                    return NativeNodeError::new("Error", format!("Unsupported libzstd error code: {number}"));
                };
                (number as i32, *name)
            }
        };
        NativeNodeError::new("Error", self.to_string())
            .with_property("errno", JsValue::Number(f64::from(errno)))
            .with_property("code", JsValue::String(code.into()))
    }
}

// zstd_errors.h from the existing pinned libzstd 1.5.7 dependency; these are
// Node's ZSTD error property names, not messages reconstructed from Rust text.
const ZSTD_ERROR_CODES: &[(usize, &str)] = &[
    (1, "ZSTD_error_GENERIC"),
    (10, "ZSTD_error_prefix_unknown"),
    (12, "ZSTD_error_version_unsupported"),
    (14, "ZSTD_error_frameParameter_unsupported"),
    (16, "ZSTD_error_frameParameter_windowTooLarge"),
    (20, "ZSTD_error_corruption_detected"),
    (22, "ZSTD_error_checksum_wrong"),
    (24, "ZSTD_error_literals_headerWrong"),
    (30, "ZSTD_error_dictionary_corrupted"),
    (32, "ZSTD_error_dictionary_wrong"),
    (34, "ZSTD_error_dictionaryCreation_failed"),
    (40, "ZSTD_error_parameter_unsupported"),
    (41, "ZSTD_error_parameter_combination_unsupported"),
    (42, "ZSTD_error_parameter_outOfBound"),
    (44, "ZSTD_error_tableLog_tooLarge"),
    (46, "ZSTD_error_maxSymbolValue_tooLarge"),
    (48, "ZSTD_error_maxSymbolValue_tooSmall"),
    (49, "ZSTD_error_cannotProduce_uncompressedBlock"),
    (50, "ZSTD_error_stabilityCondition_notRespected"),
    (60, "ZSTD_error_stage_wrong"),
    (62, "ZSTD_error_init_missing"),
    (64, "ZSTD_error_memory_allocation"),
    (66, "ZSTD_error_workSpace_tooSmall"),
    (70, "ZSTD_error_dstSize_tooSmall"),
    (72, "ZSTD_error_srcSize_wrong"),
    (74, "ZSTD_error_dstBuffer_null"),
    (80, "ZSTD_error_noForwardProgress_destFull"),
    (82, "ZSTD_error_noForwardProgress_inputEmpty"),
    (100, "ZSTD_error_frameIndex_tooLarge"),
    (102, "ZSTD_error_seekableIO"),
    (104, "ZSTD_error_dstBuffer_wrong"),
    (105, "ZSTD_error_srcBuffer_wrong"),
    (106, "ZSTD_error_sequenceProducer_failed"),
    (107, "ZSTD_error_externalSequences_invalid"),
];
