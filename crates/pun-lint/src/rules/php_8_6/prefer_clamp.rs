use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{Argument, Call, Expression, FunctionCall, Literal, Node, NodeKind};
use pun_tokens::php_compat::float_to_string;
use std::cmp::Ordering;

use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::{args, named};
use crate::rules::support::values::{Constant, constant};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferClamp;

impl Rule for PreferClamp {
    fn id(&self) -> &'static str {
        "lint/php-8.6/prefer-clamp"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80600)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(o) = node else { return };
        let outer = o;
        let (outer_is_max, inner_name) = if named(outer, "max") {
            (true, "min")
        } else if named(outer, "min") {
            (false, "max")
        } else {
            return;
        };
        let outer_args = args(outer);
        let [left, right] = outer_args[..] else {
            return;
        };
        if !supported(left) || !supported(right) {
            return;
        }
        let at = node_span(outer).start;
        let (inner, outer_bound) = match (inner_call(value(left)), inner_call(value(right))) {
            (Some(inner), _) => (inner, value(right)),
            (None, Some(inner)) => (inner, value(left)),
            (None, None) => return,
        };
        if !named(inner, inner_name) {
            return;
        }
        let inner_args = args(inner);
        let [first, second] = inner_args[..] else {
            return;
        };
        if !supported(first) || !supported(second) {
            return;
        }
        let known = |e: &Expression<'_>| constant(ancestors, e, at).is_some();
        let (value_expr, inner_bound) = match (known(value(first)), known(value(second))) {
            (true, false) => (value(second), value(first)),
            (false, true) => (value(first), value(second)),
            _ => return,
        };
        let text = |e: &Expression<'_>| {
            let e = unparenthesize(e);
            &ctx.source[node_span(e).start..node_span(e).end]
        };
        let (low, high) = if outer_is_max {
            (outer_bound, inner_bound)
        } else {
            (inner_bound, outer_bound)
        };
        // Rector 2.9.2: clamp() throws on min > max or a NAN bound, so both bounds must be known and ordered.
        let (Some(min), Some(max)) = (bound(ancestors, low, at), bound(ancestors, high, at)) else {
            return;
        };
        if less_or_equal(&min, &max) != Some(true) {
            return;
        }
        let span = node_span(outer);
        out.push(Report {
            span,
            message: "Use `clamp()` instead of nested `min()` and `max()`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(
                    span,
                    format!("clamp({}, {}, {})", text(value_expr), text(low), text(high)),
                )],
            }),
        });
    }
}

/// A bound's constant value; an integer literal past `PHP_INT_MAX` is a float, as in PHP.
fn bound(ancestors: &[Node<'_, '_>], expr: &Expression<'_>, at: usize) -> Option<Constant> {
    if let Expression::Literal(Literal::Integer(i)) = unparenthesize(expr) {
        return Some(match i.value.and_then(|v| i64::try_from(v).ok()) {
            Some(v) => Constant::Int(v),
            None => Constant::Float(literal_float(i.raw)),
        });
    }
    constant(ancestors, expr, at)
}

fn literal_float(raw: &[u8]) -> f64 {
    let digits: Vec<u8> = raw.iter().copied().filter(|&b| b != b'_').collect();
    let (radix, digits) = match digits.as_slice() {
        [b'0', b'x' | b'X', rest @ ..] => (16, rest),
        [b'0', b'b' | b'B', rest @ ..] => (2, rest),
        [b'0', b'o' | b'O', rest @ ..] => (8, rest),
        [b'0', rest @ ..] if !rest.is_empty() => (8, rest),
        all => (10, all),
    };
    (digits.iter())
        .filter_map(|&d| char::from(d).to_digit(radix))
        .fold(0.0, |acc, d| acc * f64::from(radix) + f64::from(d))
}

/// PHP 8's `$min <= $max` on two constants; `None` when NAN makes it unordered.
fn less_or_equal(min: &Constant, max: &Constant) -> Option<bool> {
    compare(min, max).map(|o| o != Ordering::Greater)
}

fn compare(a: &Constant, b: &Constant) -> Option<Ordering> {
    match (a, b) {
        (Constant::Null, Constant::String(s)) => Some(b"".as_slice().cmp(s)),
        (Constant::String(s), Constant::Null) => Some(s.as_slice().cmp(b"")),
        (Constant::Null | Constant::Bool(_), _) | (_, Constant::Null | Constant::Bool(_)) => {
            Some(truthy(a).cmp(&truthy(b)))
        }
        (Constant::String(x), Constant::String(y)) => match (numeric(x), numeric(y)) {
            (Some(x), Some(y)) => x.partial_cmp(&y),
            _ => Some(x.cmp(y)),
        },
        (Constant::String(_), _) => compare(b, a).map(Ordering::reverse),
        (n, Constant::String(s)) => match numeric(s) {
            Some(y) => number(n)?.partial_cmp(&y),
            None => Some(as_string(n).as_slice().cmp(s)),
        },
        (Constant::Int(x), Constant::Int(y)) => Some(x.cmp(y)),
        _ => number(a)?.partial_cmp(&number(b)?),
    }
}

fn truthy(c: &Constant) -> bool {
    match c {
        Constant::Null => false,
        Constant::Bool(b) => *b,
        Constant::Int(i) => *i != 0,
        Constant::Float(f) => *f != 0.0,
        Constant::String(s) => !(s.is_empty() || s == b"0"),
    }
}

fn number(c: &Constant) -> Option<f64> {
    match c {
        Constant::Int(i) => Some(*i as f64),
        Constant::Float(f) if !f.is_nan() => Some(*f),
        _ => None,
    }
}

fn as_string(c: &Constant) -> Vec<u8> {
    match c {
        Constant::Int(i) => i.to_string().into_bytes(),
        Constant::Float(f) => float_to_string(*f).into_bytes(),
        _ => vec![],
    }
}

/// A numeric string's value (PHP 8: surrounding whitespace, a sign, digits, a fraction and an exponent).
fn numeric(s: &[u8]) -> Option<f64> {
    let s = std::str::from_utf8(s)
        .ok()?
        .trim_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mantissa_ok = (digits(int) || int.is_empty())
        && (frac.is_empty() || digits(frac))
        && (digits(int) || digits(frac));
    let exponent_ok = exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)));
    (mantissa_ok && exponent_ok)
        .then(|| s.parse().ok())
        .flatten()
}

fn supported(arg: &Argument<'_>) -> bool {
    !unpacked(arg) && matches!(arg, Argument::Positional(_))
}

fn inner_call<'a>(expr: &'a Expression<'a>) -> Option<&'a FunctionCall<'a>> {
    match unparenthesize(expr) {
        Expression::Call(Call::Function(call)) => Some(call),
        _ => None,
    }
}
