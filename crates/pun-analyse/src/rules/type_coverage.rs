//! tomasvotruba/type-coverage 2.5.1's rules: the `analyse/type-coverage/*` rules, which run with
//! `[extensions] type-coverage = "on"` (`pun_config::extensions::TYPE_COVERAGE`, which owns the `type-coverage`
//! group).
//!
//! Each file records what its four collectors (`src/Collectors/*TypeDeclarationCollector.php`) collect as a
//! `typeCoverage.collected` pseudo-error, and [`finish`] sums them over the project as the rules
//! (`src/Rules/*TypeCoverageRule.php`) and `CollectorDataNormalizer` do. A type is missing when the declaration has
//! no native type; PHPDoc counts only in the collectors' raw substring checks. The constant rule runs on every PHP
//! version Pun supports (8.3 and later). Measure mode's errors have no identifier and no file (PHPStan's `N/A`, line
//! -1), and `Checks::keep_reported` keeps them.

use std::collections::{BTreeMap, HashSet};

use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use mago_word::word;
use pun_phpstan::ignore::Error;
use pun_phpstan::{Array, Value};

use super::Checker;

pub(crate) const COLLECTED: &str = "typeCoverage.collected";

/// Return, param, property and constant: each rule's identifier, the noun of its message and its measure message's
/// subject.
const KINDS: [(&str, &str, &str); 4] = [
    ("typeCoverage.returnTypeCoverage", "return", "Return"),
    ("typeCoverage.paramTypeCoverage", "param", "Param"),
    ("typeCoverage.propertyTypeCoverage", "property", "Property"),
    (
        "typeCoverage.constantTypeCoverage",
        "constant",
        "Class constant",
    ),
];
const RETURN: usize = 0;
const PARAM: usize = 1;
const PROPERTY: usize = 2;
const CONSTANT: usize = 3;

/// `src/Configuration.php`: the required coverage of each kind (return, param, property, constant), `None` when Pun
/// runs its rule off, and measure mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeCoverage {
    pub levels: [Option<f64>; 4],
    pub measure: bool,
}

impl TypeCoverage {
    /// The `type_coverage` parameters: a kind's alias (`return`) wins over its `_type` key (`return_type`) when it is
    /// not null. A kind with neither is off; none on is no type coverage at all.
    pub fn from_params(params: &Array) -> Option<TypeCoverage> {
        let p = params.get_str("type_coverage")?.as_array()?;
        let number = |k: &str| match p.get_str(k) {
            Some(Value::Int(i)) => Some(*i as f64),
            Some(Value::Float(f)) => Some(*f),
            _ => None,
        };
        let levels =
            KINDS.map(|(_, noun, _)| number(noun).or_else(|| number(&format!("{noun}_type"))));
        levels.iter().any(Option::is_some).then(|| TypeCoverage {
            levels,
            measure: p.get_str("measure") == Some(&Value::Bool(true)),
        })
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// ClassMethod::isMagic (nikic/php-parser 5).
const MAGIC: [&str; 17] = [
    "__construct",
    "__destruct",
    "__call",
    "__callstatic",
    "__get",
    "__set",
    "__isset",
    "__unset",
    "__sleep",
    "__wakeup",
    "__tostring",
    "__set_state",
    "__clone",
    "__invoke",
    "__debuginfo",
    "__serialize",
    "__unserialize",
];

impl<'a> Checker<'a, '_> {
    fn collecting(&self, kind: usize) -> bool {
        !self.outside_target_trait() && self.type_coverage.is_some_and(|t| t.levels[kind].is_some())
    }

    /// One collector's data: `count` declarations, the lines of those missing a type, and for a trait member the
    /// declaration's start (`startFilePos`), which counts it once however many classes use the trait.
    fn collect_coverage(&mut self, kind: usize, count: usize, missing: &[i64], at: Option<Span>) {
        let (file, key) = match (&self.trait_context, at) {
            (Some(_), Some(at)) => (self.file_name(), at.start.offset.to_string()),
            (Some(t), None) => (t.class_file.clone(), String::new()),
            (None, _) => (self.file_name(), String::new()),
        };
        let lines: Vec<String> = missing.iter().map(i64::to_string).collect();
        self.errors.push(Error {
            identifier: Some(COLLECTED.to_string()),
            ..Error::new(
                format!("{kind}\u{1f}{count}\u{1f}{key}\u{1f}{}", lines.join(",")),
                file,
                None,
            )
        });
    }

    fn doc_contains(&self, span: Span, needles: &[&[u8]]) -> bool {
        self.doc_before(self.file.id, span.start.offset)
            .is_some_and(|(raw, _)| needles.iter().any(|n| contains(raw, n)))
    }

    fn coverage_class(&self) -> Option<&'a ClassLikeMetadata> {
        let name = self.classes.last()?.name.as_ref()?;
        self.codebase.get_class_like(name.as_bytes())
    }

    fn parents(&self, k: &ClassLikeMetadata) -> impl Iterator<Item = &'a ClassLikeMetadata> {
        let codebase = self.codebase;
        (k.all_parent_classes.iter()).filter_map(move |p| codebase.get_class_like(p.as_bytes()))
    }
}

/// ReturnTypeDeclarationCollector: every method but the magic ones and a trait's `__construct` under any alias.
pub(super) fn method(c: &mut Checker, m: &Method, name: &str) {
    if !c.collecting(RETURN)
        || MAGIC.contains(&name.to_ascii_lowercase().as_str())
        || (c.trait_context.is_some() && m.name.value.eq_ignore_ascii_case(b"__construct"))
    {
        return;
    }
    let missing: &[i64] = if m.return_type_hint.is_none() {
        &[c.line_of(m.span())]
    } else {
        &[]
    };
    c.collect_coverage(RETURN, 1, missing, Some(m.span()));
}

/// ParamTypeDeclarationCollector on a function, method, closure, arrow function or property hook (`method` names the
/// method): none without parameters, a docblock with `@param callable`, or a method a parent class or interface
/// declares. Variadic parameters don't count.
pub(super) fn parameters(
    c: &mut Checker,
    at: Span,
    list: &FunctionLikeParameterList,
    method: Option<&str>,
) {
    if !c.collecting(PARAM)
        || list.parameters.is_empty()
        || c.doc_contains(at, &[b"@param callable"])
    {
        return;
    }
    if let (Some(name), Some(k)) = (method, c.coverage_class()) {
        let lower = word(name.to_ascii_lowercase());
        let codebase = c.codebase;
        let interfaces =
            (k.all_parent_interfaces.iter()).filter_map(|i| codebase.get_class_like(i.as_bytes()));
        if c.parents(k).chain(interfaces).any(|a| {
            a.appearing_method_ids.contains_key(&lower)
                || a.pseudo_methods.contains(&lower)
                || a.static_pseudo_methods.contains(&lower)
        }) {
            return;
        }
    }
    let params: Vec<_> = (list.parameters.iter())
        .filter(|p| p.ellipsis.is_none())
        .collect();
    let missing: Vec<i64> = (params.iter())
        .filter(|p| p.hint.is_none())
        .map(|p| c.line_of(p.span()))
        .collect();
    c.collect_coverage(PARAM, params.len(), &missing, Some(at));
}

/// PropertyTypeDeclarationCollector on a class-like's own property statements (never a trait's): one guarded by a
/// parent class's property of its first name, or with `callable` or `resource` in its docblock, has a type.
pub(super) fn properties(c: &mut Checker, members: &[ClassLikeMember]) {
    if !c.collecting(PROPERTY) {
        return;
    }
    let class = c.coverage_class();
    let mut count = 0;
    let mut missing = Vec::new();
    for member in members {
        let ClassLikeMember::Property(p) = member else {
            continue;
        };
        count += 1;
        let name = word(p.first_variable().name);
        let guarded = class.is_some_and(|k| {
            c.parents(k)
                .any(|a| a.appearing_property_ids.contains_key(&name))
        });
        if !guarded && p.hint().is_none() && !c.doc_contains(p.span(), &[b"callable", b"resource"])
        {
            missing.push(c.line_of(p.span()));
        }
    }
    c.collect_coverage(PROPERTY, count, &missing, None);
}

/// ConstantTypeDeclarationCollector, one constant statement at a time: ClassConstantsNode holds the class's own and
/// its traits' constants, all counted for the class (in its file, at the trait's line), and one guarded by a parent
/// class's constant of its first name has a type.
pub(super) fn constant(c: &mut Checker, k: &ClassLikeConstant) {
    if !c.collecting(CONSTANT) {
        return;
    }
    let name = word(k.first_item().name.value);
    let guarded = c.coverage_class().is_some_and(|class| {
        let codebase = c.codebase;
        c.parents(class).any(|p| {
            p.constants.contains_key(&name)
                || (p.all_parent_interfaces.iter())
                    .filter_map(|i| codebase.get_class_like(i.as_bytes()))
                    .any(|i| i.constants.contains_key(&name))
        })
    });
    let missing: &[i64] = if guarded || k.hint.is_some() {
        &[]
    } else {
        &[c.line_of(k.span())]
    };
    c.collect_coverage(CONSTANT, 1, missing, None);
}

/// TypeCountAndMissingTypes::getCoveragePercentage: rounded down to one decimal.
fn percentage(total: usize, missing: usize) -> f64 {
    if total == 0 {
        return 100.0;
    }
    let relative = 100.0 * ((total - missing) as f64 / total as f64);
    (relative * 10.0).floor() / 10.0
}

/// The four rules on the collected data (AbstractTypeCoverageRule::processNode and TypeCoverageFormatter): measure
/// mode's one error per rule, else one per missing line while the coverage is below the required level. The
/// project-wide errors then meet each file's `@phpstan-ignore` comments.
pub(crate) fn finish(
    coverage: Option<&TypeCoverage>,
    errors: Vec<Error>,
    sources: &crate::codebase::Sources,
    explain_ignores: bool,
) -> Vec<Error> {
    let (collected, mut errors): (Vec<Error>, Vec<Error>) = errors
        .into_iter()
        .partition(|e| e.identifier.as_deref() == Some(COLLECTED));
    let Some(coverage) = coverage else {
        return errors;
    };
    let mut joined: BTreeMap<String, Vec<Error>> = BTreeMap::new();
    for (kind, (identifier, noun, subject)) in KINDS.iter().enumerate() {
        let Some(level) = coverage.levels[kind] else {
            continue;
        };
        let (mut total, mut missing) = (0, 0);
        let mut lines: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
        let mut seen = HashSet::new();
        for e in &collected {
            let fields: Vec<&str> = e.message.split('\u{1f}').collect();
            let [k, count, key, missing_lines] =
                <[&str; 4]>::try_from(fields).expect("a record has four fields");
            if k != kind.to_string() || (!key.is_empty() && !seen.insert((&e.file, key))) {
                continue;
            }
            total += count.parse::<usize>().expect("a count");
            let at = lines.entry(&e.file).or_default();
            for line in missing_lines.split(',').filter(|l| !l.is_empty()) {
                at.push(line.parse().expect("a line"));
                missing += 1;
            }
        }
        let coverage_percentage = percentage(total, missing);
        if coverage.measure {
            errors.push(Error::new(
                format!(
                    "{subject} type coverage is {coverage_percentage:.1} % out of {total} possible"
                ),
                "N/A",
                Some(-1),
            ));
            continue;
        }
        if level <= 0.0 || total == 0 || coverage_percentage >= level {
            continue;
        }
        let message = format!(
            "Out of {total} possible {noun} types, only {} - {coverage_percentage:.1} % actually have it. Add more {noun} types to get over {level} %",
            total - missing
        );
        for (file, lines) in lines {
            for line in lines {
                joined.entry(file.to_string()).or_default().push(Error {
                    identifier: Some((*identifier).to_string()),
                    ..Error::new(message.clone(), file, Some(line))
                });
            }
        }
    }
    super::named_argument::ignore_in_files(&mut errors, joined, sources, explain_ignores);
    errors
}
