//! Port of node-semver 7.8.0's strict/default operations used by package-manager.
//! Source: npm/node-semver efa4be6096c1f9b77d9d27d6132f6220c43b4e31.
//! ISC copyright Isaac Z. Schlueter and Contributors; full notice:
//! docs/porting/vendor/node-semver-LICENSE.

use std::cmp::Ordering;
use std::sync::LazyLock;

use pi_js_compat::array_sort::js_array_sort;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::{JsCaptures, JsRegex};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use pi_js_compat::number::{js_number_to_string, parse_number};

const MAX_LENGTH: usize = 256;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

// Exact default safeSrc patterns from internal/re.js; only global replacements
// from upstream are applied globally below.
static FULL: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"^v?(0|[1-9]\d{0,256})\.(0|[1-9]\d{0,256})\.(0|[1-9]\d{0,256})(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?$"#).expect("pinned FULL regexp")
});
static COMPARATOR: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"^((?:<|>)?=?)\s{0,1}(v?(0|[1-9]\d{0,256})\.(0|[1-9]\d{0,256})\.(0|[1-9]\d{0,256})(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)$|^$"#).expect("pinned COMPARATOR regexp")
});
static HYPHENRANGE: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"^\s{0,1}([v=\s]*(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*)))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)?)?)\s{1,1}-\s{1,1}([v=\s]*(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*)))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)?)?)\s{0,1}$"#).expect("pinned HYPHENRANGE regexp")
});
static COMPARATORTRIM: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"(\s{0,1})((?:<|>)?=?)\s{0,1}([v=\s]*(\d{1,256})\.(\d{1,256})\.(\d{1,256})(?:-?((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|\d{1,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|\d{1,256}))*))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?|[v=\s]*(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*)))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)?)?)"#).expect("pinned COMPARATORTRIM regexp")
});
static TILDETRIM: LazyLock<JsRegex> =
    LazyLock::new(|| JsRegex::new(r#"(\s{0,1})(?:~>?)\s{1,1}"#).expect("pinned TILDETRIM regexp"));
static CARETTRIM: LazyLock<JsRegex> =
    LazyLock::new(|| JsRegex::new(r#"(\s{0,1})(?:\^)\s{1,1}"#).expect("pinned CARETTRIM regexp"));
static CARET: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"^(?:\^)[v=\s]*(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*)))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)?)?$"#).expect("pinned CARET regexp")
});
static TILDE: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"^(?:~>?)[v=\s]*(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*)))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)?)?$"#).expect("pinned TILDE regexp")
});
static XRANGE: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"^((?:<|>)?=?)\s{0,1}[v=\s]*(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:\.(0|[1-9]\d{0,256}|x|X|\*)(?:(?:-((?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256})(?:\.(?:\d{0,256}[a-zA-Z-][a-zA-Z0-9-]{0,250}|0|[1-9]\d{0,256}))*)))?(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))?)?)?$"#).expect("pinned XRANGE regexp")
});
static STAR: LazyLock<JsRegex> = LazyLock::new(|| JsRegex::new(r#"(<|>)?=?\s{0,1}\*"#).expect("pinned STAR regexp"));
static GTE0: LazyLock<JsRegex> =
    LazyLock::new(|| JsRegex::new(r#"^\s{0,1}>=\s{0,1}0\.0\.0\s{0,1}$"#).expect("pinned GTE0 regexp"));

static BUILD: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r#"(?:\+([a-zA-Z0-9-]{1,250}(?:\.[a-zA-Z0-9-]{1,250})*))"#).expect("pinned BUILD regexp")
});

fn type_error(message: impl Into<JsString>) -> NativeNodeError {
    NativeNodeError::from_constructor(NativeErrorClass::TypeError, message)
}
fn capture(captures: &JsCaptures, index: usize) -> Option<JsString> {
    captures.get(index).map(|value| value.text())
}
fn numeric(text: &JsString) -> f64 {
    parse_number(&text.to_utf8().expect("semver numeric captures are ASCII"))
}
fn increment(text: &JsString) -> JsString {
    js_number_to_string(numeric(text) + 1.0).into()
}
fn collapse_spaces(text: &JsString) -> JsString {
    static SPACES: LazyLock<JsRegex> = LazyLock::new(|| JsRegex::new(r"\s+").expect("space regexp"));
    SPACES.replace_all(text.trim(), " ")
}
fn split_spaces(text: &JsString) -> Vec<JsString> {
    collapse_spaces(text).split(' ').collect()
}

#[derive(Clone)]
struct SemVer {
    core: [u64; 3],
    prerelease: Vec<JsString>,
    version: JsString,
}
impl SemVer {
    // classes/semver.js:10-92 (only strict string inputs).
    fn parse(raw: &JsString) -> Result<Self, NativeNodeError> {
        if raw.len() > MAX_LENGTH {
            return Err(type_error("version is longer than 256 characters"));
        }
        let captures = FULL
            .captures(raw.trim())
            .ok_or_else(|| type_error(js_format!("Invalid Version: {}", raw)))?;
        let core = Self::parse_core(&captures)?;
        let prerelease: Vec<_> = capture(&captures, 4).map_or_else(Vec::new, |value| value.split('.').collect());
        let mut version = js_format!("{}.{}.{}", core[0], core[1], core[2]);
        if let Some(pre) = capture(&captures, 4) {
            version = js_format!("{}-{}", version, pre);
        }
        Ok(Self {
            core,
            prerelease,
            version,
        })
    }
    fn parse_core(captures: &JsCaptures) -> Result<[u64; 3], NativeNodeError> {
        let mut core = [0; 3];
        for (index, name) in ["major", "minor", "patch"].iter().enumerate() {
            let number = numeric(&capture(captures, index + 1).expect("FULL component"));
            if !(0.0..=MAX_SAFE_INTEGER).contains(&number) {
                return Err(type_error(format!("Invalid {name} version")));
            }
            core[index] = number as u64;
        }
        Ok(core)
    }
    // classes/semver.js:93-165, internal/identifiers.js:3-24.
    fn compare(&self, other: &Self) -> Ordering {
        let core_order = self.core.cmp(&other.core);
        if core_order != Ordering::Equal {
            return core_order;
        }
        if self.prerelease.is_empty() || other.prerelease.is_empty() {
            return self.prerelease.is_empty().cmp(&other.prerelease.is_empty());
        }
        for (left, right) in self.prerelease.iter().zip(&other.prerelease) {
            let order = compare_identifiers(left, right);
            if order != Ordering::Equal {
                return order;
            }
        }
        self.prerelease.len().cmp(&other.prerelease.len())
    }
}
fn compare_identifiers(left: &JsString, right: &JsString) -> Ordering {
    let digits = u16::from(b'0')..=u16::from(b'9');
    let left_numeric = left.units().iter().all(|unit| digits.contains(unit));
    let right_numeric = right.units().iter().all(|unit| digits.contains(unit));
    match (left_numeric, right_numeric) {
        (true, true) => numeric(left)
            .partial_cmp(&numeric(right))
            .expect("numeric identifiers are not NaN"),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(right),
    }
}

#[derive(Clone)]
struct Comparator {
    operator: JsString,
    semver: Option<SemVer>,
    value: JsString,
}
impl Comparator {
    // classes/comparator.js:10-57.
    fn parse(raw: &JsString) -> Result<Self, NativeNodeError> {
        let text = collapse_spaces(raw);
        let matched = COMPARATOR
            .captures(&text)
            .ok_or_else(|| type_error(js_format!("Invalid comparator: {}", text)))?;
        let operator = capture(&matched, 1).unwrap_or_default();
        let operator = if operator == "=" { JsString::default() } else { operator };
        let semver = capture(&matched, 2)
            .map(|version| SemVer::parse(&version))
            .transpose()?;
        let value = semver.as_ref().map_or_else(JsString::default, |version| {
            js_format!("{}{}", operator, version.version)
        });
        Ok(Self {
            operator,
            semver,
            value,
        })
    }
    fn test(&self, version: &SemVer) -> bool {
        let Some(target) = &self.semver else {
            return true;
        };
        let order = version.compare(target);
        match self.operator.to_utf8().expect("comparator operator is ASCII").as_str() {
            "" => order.is_eq(),
            ">" => order.is_gt(),
            ">=" => !order.is_lt(),
            "<" => order.is_lt(),
            "<=" => !order.is_gt(),
            _ => unreachable!("COMPARATOR operator grammar"),
        }
    }
}

struct Range {
    sets: Vec<Vec<Comparator>>,
}
impl Range {
    // classes/range.js:6-99 (default options; memoization is not observable).
    fn parse(raw: &JsString) -> Result<Self, NativeNodeError> {
        let raw = collapse_spaces(raw);
        let mut sets = split_or(&raw)
            .iter()
            .map(|branch| parse_range(&branch.trim()))
            .collect::<Result<Vec<_>, _>>()?;
        if sets.len() > 1 {
            let first = sets[0].clone();
            sets.retain(|set| set[0].value != "<0.0.0-0");
            if sets.is_empty() {
                sets.push(first);
            } else if let Some(any) = sets.iter().find(|set| set.len() == 1 && set[0].semver.is_none()) {
                sets = vec![any.clone()];
            }
        }
        Ok(Self { sets })
    }
    fn format(&self) -> JsString {
        let branches: Vec<_> = self
            .sets
            .iter()
            .map(|set| join(&set.iter().map(|cmp| cmp.value.clone()).collect::<Vec<_>>(), " "))
            .collect();
        join(&branches, "||")
    }
    fn test(&self, version: &JsString) -> bool {
        let Ok(version) = SemVer::parse(version) else {
            return false;
        };
        self.sets.iter().any(|set| test_set(set, &version))
    }
}
fn split_or(text: &JsString) -> Vec<JsString> {
    let mut branches = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index + 1 < text.len() {
        if text.units()[index..].starts_with(&[124, 124]) {
            branches.push(text.slice(start, index));
            index += 2;
            start = index;
        } else {
            index += 1;
        }
    }
    branches.push(text.slice(start, text.len()));
    branches
}
fn join(parts: &[JsString], separator: &str) -> JsString {
    parts
        .iter()
        .enumerate()
        .fold(JsString::default(), |result, (index, part)| {
            if index == 0 {
                part.clone()
            } else {
                js_format!("{}{}{}", result, separator, part)
            }
        })
}
// classes/range.js:101-174.
fn parse_range(raw: &JsString) -> Result<Vec<Comparator>, NativeNodeError> {
    let hyphen = HYPHENRANGE.replace_all_with(raw, replace_hyphen);
    let comparator = COMPARATORTRIM.replace_all(hyphen, "$1$2$3");
    let tilde = TILDETRIM.replace_all(comparator, "$1~");
    let caret = CARETTRIM.replace_all(tilde, "$1^");
    let expanded: Vec<_> = caret.split(' ').map(|part| expand_comparator(&part)).collect();
    let comparators = split_spaces(&join(&expanded, " "))
        .iter()
        .map(|part| Comparator::parse(&GTE0.replace_all(part.trim(), "")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut unique = indexmap::IndexMap::new();
    for comparator in comparators {
        if comparator.value == "<0.0.0-0" {
            return Ok(vec![comparator]);
        }
        unique.insert(comparator.value.clone(), comparator);
    }
    if unique.len() > 1 {
        unique.shift_remove(&JsString::default());
    }
    Ok(unique.into_values().collect())
}
fn test_set(set: &[Comparator], version: &SemVer) -> bool {
    if !set.iter().all(|comparator| comparator.test(version)) {
        return false;
    }
    if version.prerelease.is_empty() {
        return true;
    }
    set.iter()
        .filter_map(|comparator| comparator.semver.as_ref())
        .any(|allowed| !allowed.prerelease.is_empty() && allowed.core == version.core)
}
fn remove_first(pattern: &JsRegex, text: &JsString) -> JsString {
    if let Some(matched) = pattern.find(text) {
        js_format!(
            "{}{}",
            text.slice(0, matched.start()),
            text.slice(matched.end(), text.len())
        )
    } else {
        text.clone()
    }
}
fn expand_comparator(text: &JsString) -> JsString {
    let without_build = remove_first(&BUILD, text);
    let caret = CARET.replace_all_with(without_build, replace_caret);
    let tilde = TILDE.replace_all_with(caret, replace_tilde);
    let xrange = XRANGE.replace_all_with(tilde, replace_xrange);
    remove_first(&STAR, &xrange.trim())
}

struct PartialVersion {
    major: Option<JsString>,
    minor: Option<JsString>,
    patch: Option<JsString>,
    prerelease: Option<JsString>,
}
impl PartialVersion {
    fn from_captures(captures: &JsCaptures, first: usize) -> Self {
        Self {
            major: capture(captures, first),
            minor: capture(captures, first + 1),
            patch: capture(captures, first + 2),
            prerelease: capture(captures, first + 3),
        }
    }
    fn major(&self) -> &JsString {
        self.major.as_ref().expect("XRANGE major capture")
    }
    fn minor(&self) -> &JsString {
        self.minor.as_ref().expect("non-wildcard minor capture")
    }
    fn patch(&self) -> &JsString {
        self.patch.as_ref().expect("non-wildcard patch capture")
    }
    fn lower(&self) -> JsString {
        let version = js_format!("{}.{}.{}", self.major(), self.minor(), self.patch());
        if let Some(pre) = &self.prerelease {
            js_format!("{}-{}", version, pre)
        } else {
            version
        }
    }
}
fn is_x(value: Option<&JsString>) -> bool {
    value.is_none_or(|value| ["", "x", "X", "*"].iter().any(|pattern| value == *pattern))
}
// classes/range.js:278-312.
fn replace_tilde(captures: &JsCaptures) -> JsString {
    let version = PartialVersion::from_captures(captures, 1);
    if is_x(version.major.as_ref()) {
        return JsString::default();
    }
    if is_x(version.minor.as_ref()) {
        return js_format!(">={}.0.0 <{}.0.0-0", version.major(), increment(version.major()));
    }
    let upper = js_format!("{}.{}.0-0", version.major(), increment(version.minor()));
    let lower = if is_x(version.patch.as_ref()) {
        js_format!("{}.{}.0", version.major(), version.minor())
    } else {
        version.lower()
    };
    js_format!(">={} <{}", lower, upper)
}
// classes/range.js:329-384.
fn replace_caret(captures: &JsCaptures) -> JsString {
    let version = PartialVersion::from_captures(captures, 1);
    if is_x(version.major.as_ref()) {
        return JsString::default();
    }
    let upper = caret_upper(&version);
    let lower = if is_x(version.minor.as_ref()) {
        js_format!("{}.0.0", version.major())
    } else if is_x(version.patch.as_ref()) {
        js_format!("{}.{}.0", version.major(), version.minor())
    } else {
        version.lower()
    };
    js_format!(">={} <{}", lower, upper)
}
fn caret_upper(version: &PartialVersion) -> JsString {
    if is_x(version.minor.as_ref()) || version.major() != "0" {
        return js_format!("{}.0.0-0", increment(version.major()));
    }
    if is_x(version.patch.as_ref()) || version.minor() != "0" {
        return js_format!("0.{}.0-0", increment(version.minor()));
    }
    js_format!("0.0.{}-0", increment(version.patch()))
}
// classes/range.js:398-466.
fn replace_xrange(captures: &JsCaptures) -> JsString {
    let version = PartialVersion::from_captures(captures, 2);
    let mut operator = capture(captures, 1).expect("XRANGE operator capture");
    let major_x = is_x(version.major.as_ref());
    let minor_x = major_x || is_x(version.minor.as_ref());
    let patch_x = minor_x || is_x(version.patch.as_ref());
    if operator == "=" && patch_x {
        operator = JsString::default();
    }
    if major_x {
        return if operator == ">" || operator == "<" {
            "<0.0.0-0".into()
        } else {
            "*".into()
        };
    }
    if !operator.is_empty() && patch_x {
        return xrange_operator(&version, &operator, minor_x);
    }
    if minor_x {
        return js_format!(">={}.0.0 <{}.0.0-0", version.major(), increment(version.major()));
    }
    if patch_x {
        return js_format!(
            ">={}.{}.0 <{}.{}.0-0",
            version.major(),
            version.minor(),
            version.major(),
            increment(version.minor())
        );
    }
    capture(captures, 0).expect("whole XRANGE match")
}
fn xrange_operator(version: &PartialVersion, operator: &JsString, minor_x: bool) -> JsString {
    let major = version.major().clone();
    let minor = if minor_x { "0".into() } else { version.minor().clone() };
    let (operator, major, minor) = if operator == ">" || operator == "<=" {
        let bound = if minor_x {
            (increment(&major), "0".into())
        } else {
            (major, increment(&minor))
        };
        (if operator == ">" { ">=" } else { "<" }, bound.0, bound.1)
    } else {
        (if operator == "<" { "<" } else { ">=" }, major, minor)
    };
    let suffix = if operator == "<" { "-0" } else { "" };
    js_format!("{}{}.{}.0{}", operator, major, minor, suffix)
}
// classes/range.js:495-533 (includePrerelease=false).
fn replace_hyphen(captures: &JsCaptures) -> JsString {
    let from = PartialVersion::from_captures(captures, 2);
    let to = PartialVersion::from_captures(captures, 8);
    let lower = if is_x(from.major.as_ref()) {
        JsString::default()
    } else if is_x(from.minor.as_ref()) {
        js_format!(">={}.0.0", from.major())
    } else if is_x(from.patch.as_ref()) {
        js_format!(">={}.{}.0", from.major(), from.minor())
    } else {
        js_format!(">={}", capture(captures, 1).expect("HYPHEN from"))
    };
    let upper = if is_x(to.major.as_ref()) {
        JsString::default()
    } else if is_x(to.minor.as_ref()) {
        js_format!("<{}.0.0-0", increment(to.major()))
    } else if is_x(to.patch.as_ref()) {
        js_format!("<{}.{}.0-0", to.major(), increment(to.minor()))
    } else if to.prerelease.is_some() {
        js_format!("<={}", to.lower())
    } else {
        js_format!("<={}", capture(captures, 7).expect("HYPHEN to"))
    };
    js_format!("{} {}", lower, upper).trim()
}

pub(super) fn node_version_range(version: Option<&JsString>) -> Option<JsString> {
    let version = version.filter(|version| !version.is_empty())?;
    let formatted = Range::parse(version).ok()?.format();
    Some(if formatted.is_empty() { "*".into() } else { formatted })
}
pub(super) fn is_exact_npm_version(version: Option<&JsString>) -> bool {
    version.is_some_and(|version| SemVer::parse(version).is_ok())
}
pub(super) fn installed_version_satisfies(version: &JsString, range: &JsString) -> bool {
    Range::parse(range).is_ok_and(|range| range.test(version))
}
pub(super) fn max_satisfying(versions: &[JsString], range: &JsString) -> Option<JsString> {
    let range = Range::parse(range).ok()?;
    let mut maximum: Option<(&JsString, SemVer)> = None;
    for version in versions {
        if range.test(version) {
            let parsed = SemVer::parse(version).expect("range.test parsed the version");
            if maximum.as_ref().is_none_or(|(_, best)| best.compare(&parsed).is_lt()) {
                maximum = Some((version, parsed));
            }
        }
    }
    maximum.map(|(version, _)| version.clone())
}
pub(super) fn highest_version(versions: &[JsString]) -> Result<Option<JsString>, NativeNodeError> {
    let mut sorted: Vec<_> = versions.iter().collect();
    let mut failure = None;
    // V8 callback order matters for invalid values. After its first throw, no more
    // constructors run; the infallible sorter only finishes its discarded workarray.
    js_array_sort(&mut sorted, |left, right| {
        if failure.is_some() {
            return 0.0;
        }
        match SemVer::parse(right).and_then(|right| SemVer::parse(left).map(|left| right.compare(&left))) {
            Ok(order) => match order {
                Ordering::Less => -1.0,
                Ordering::Equal => 0.0,
                Ordering::Greater => 1.0,
            },
            Err(error) => {
                failure = Some(error);
                0.0
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(sorted.first().map(|version| (*version).clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> serde_json::Value {
        serde_json::from_str(include_str!("../../tests/data/npm-semver-7.8.0.json")).unwrap()
    }
    #[test]
    fn upstream_range_parse() {
        for row in fixtures()["range-parse"].as_array().unwrap() {
            let raw: JsString = row[0].as_str().unwrap().into();
            let actual = Range::parse(&raw).ok().map(|range| {
                let text = range.format();
                if text.is_empty() { "*".into() } else { text }
            });
            let expected = row[1].as_str().map(JsString::from);
            assert_eq!(actual, expected, "{raw:?}");
        }
    }
    #[test]
    fn upstream_range_membership() {
        for (key, expected) in [("range-include", true), ("range-exclude", false)] {
            for row in fixtures()[key].as_array().unwrap() {
                let range: JsString = row[0].as_str().unwrap().into();
                let version: JsString = row[1].as_str().unwrap().into();
                assert_eq!(installed_version_satisfies(&version, &range), expected, "{row}");
                let maximum = max_satisfying(std::slice::from_ref(&version), &range);
                assert_eq!(maximum, expected.then_some(version), "maxSatisfying {row}");
            }
        }
    }
    #[test]
    fn upstream_versions() {
        for row in fixtures()["valid-versions"].as_array().unwrap() {
            let raw: JsString = row[0].as_str().unwrap().into();
            let parsed = SemVer::parse(&raw).unwrap();
            assert_eq!(
                parsed.core,
                [
                    row[1].as_u64().unwrap(),
                    row[2].as_u64().unwrap(),
                    row[3].as_u64().unwrap()
                ],
                "{row}"
            );
            assert!(is_exact_npm_version(Some(&raw)), "{row}");
        }
        for row in fixtures()["invalid-versions"].as_array().unwrap() {
            assert!(!is_exact_npm_version(Some(&row[0].as_str().unwrap().into())), "{row}");
        }
    }
    #[test]
    fn upstream_comparisons() {
        for row in fixtures()["comparisons"].as_array().unwrap() {
            let greater: JsString = row[0].as_str().unwrap().into();
            let lesser: JsString = row[1].as_str().unwrap().into();
            assert_eq!(
                SemVer::parse(&greater)
                    .unwrap()
                    .compare(&SemVer::parse(&lesser).unwrap()),
                Ordering::Greater,
                "{row}"
            );
            assert_eq!(
                highest_version(&[lesser, greater.clone()]).unwrap(),
                Some(greater),
                "{row}"
            );
        }
    }
}
