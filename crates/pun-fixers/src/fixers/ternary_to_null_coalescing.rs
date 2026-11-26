use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::{Token, Tokens};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
struct TernaryToNullCoalescingFixer {
    /// Pun's `null_checks`: also the null checks Rector's `TernaryToNullCoalescingRector` rewrites, on a plain
    /// variable: `$a !== null ? $a : $b`, `$a === null ? $b : $a` (either side), `is_null($a) ? $b : $a` and
    /// `!is_null($a) ? $a : $b`.
    null_checks: bool,
}

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(TernaryToNullCoalescingFixer::default())
}

impl Fixer for TernaryToNullCoalescingFixer {
    fn name(&self) -> &'static str {
        "ternary_to_null_coalescing"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn configure(&mut self, options: &Value) -> anyhow::Result<()> {
        let checks = match options {
            Value::Null => None,
            Value::Object(o) if o.len() == 1 => o.get("null_checks"),
            _ => anyhow::bail!("[{}] Is not configurable.", self.name()),
        };
        self.null_checks = match checks {
            None if !options.is_null() => anyhow::bail!("[{}] Is not configurable.", self.name()),
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(v) => anyhow::bail!("[{}] null_checks must be a bool, not {v}", self.name()),
        };
        Ok(())
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_token_kind_found(T_ISSET) || (self.null_checks && tokens.is_token_kind_found("?"))
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        if self.null_checks {
            for index in (0..tokens.len()).rev() {
                if tokens[index].equals("?", true) {
                    fix_null_check(tokens, index);
                }
            }
        }
        let isset_indices = tokens.find_given_kind(T_ISSET, 0, None);
        for &isset_index in isset_indices.iter().rev() {
            fix_isset(tokens, isset_index);
        }
    }
}

/// Rewrites the null check whose ternary `?` is at `question`, when its checked expression is a plain variable: `??`
/// gives the same value then, where a property or offset may call `__isset()` or `offsetExists()` instead.
fn fix_null_check(tokens: &mut Tokens, question: usize) {
    // The meaningful tokens before the `?`, nearest first, each with its first index: `\is_null` is two tokens.
    let mut before: Vec<(usize, usize)> = Vec::new();
    let mut index = question;
    while before.len() < 5 {
        let Some(prev) = tokens.get_prev_meaningful_token(index) else {
            break;
        };
        index = prev;
        if tokens[prev].is_kind(T_NS_SEPARATOR)
            && let Some(last) = before.last_mut()
            && tokens[last.0].get_content().eq_ignore_ascii_case(b"is_null")
        {
            last.1 = prev;
            continue;
        }
        before.push((prev, prev));
    }
    let is_null_word = |t: &Token| {
        t.get_content().eq_ignore_ascii_case(b"null") && t.is_kind(T_STRING)
    };
    let is_null_call = |at: &[usize]| {
        tokens[at[0]].is_kind(T_STRING)
            && tokens[at[0]].get_content().eq_ignore_ascii_case(b"is_null")
            && tokens[at[1]].equals("(", true)
            && tokens[at[2]].is_kind(T_VARIABLE)
            && tokens[at[3]].equals(")", true)
    };
    // The condition (its first token), the variable it checks, and whether that variable is the `?` branch.
    let found = [5, 4, 3].into_iter().find_map(|n| {
        let starts = before.len() >= n
            && tokens
                .get_prev_meaningful_token(before[n - 1].1)
                .is_some_and(|p| is_boundary(&tokens[p]));
        if !starts {
            return None;
        }
        let cond: Vec<usize> = before[..n].iter().rev().map(|t| t.0).collect();
        let first = before[n - 1].1;
        match n {
            5 if tokens[cond[0]].equals("!", true) && is_null_call(&cond[1..]) => {
                Some((first, cond[3], true))
            }
            4 if is_null_call(&cond) => Some((first, cond[2], false)),
            3 => {
                let [a, op, b] = [&tokens[cond[0]], &tokens[cond[1]], &tokens[cond[2]]];
                let variable = if a.is_kind(T_VARIABLE) && is_null_word(b) {
                    cond[0]
                } else if b.is_kind(T_VARIABLE) && is_null_word(a) {
                    cond[2]
                } else {
                    return None;
                };
                let is_then = op.is_kind(T_IS_NOT_IDENTICAL);
                (is_then || op.is_kind(T_IS_IDENTICAL)).then_some((cond[0], variable, is_then))
            }
            _ => None,
        }
    });
    let Some((start, variable, checked_is_then)) = found else {
        return;
    };
    let Some(colon) = ternary_colon(tokens, question) else {
        return;
    };
    let first = tokens.next_meaningful(question);
    let last = tokens.prev_meaningful(colon);
    if first > last {
        return;
    }
    let name = tokens[variable].get_content().to_vec();
    let is_variable = |i: usize| tokens[i].is_kind(T_VARIABLE) && tokens[i].get_content() == name;
    let end = if checked_is_then {
        if first != last || !is_variable(first) {
            return;
        }
        tokens.next_meaningful(colon) - 1
    } else {
        let checked = tokens.next_meaningful(colon);
        let after = tokens.get_next_meaningful_token(checked);
        if !is_variable(checked)
            || !after.is_some_and(|a| {
                tokens[a].equals_any(&[";".into(), ")".into(), ",".into(), "]".into()], true)
                    || tokens[a].is_given_kind(&[CT_ARRAY_BRACKET_CLOSE, T_CLOSE_TAG])
            })
        {
            return;
        }
        checked
    };
    if (start..=end).any(|i| tokens[i].is_comment()) {
        return;
    }
    let mut items = vec![
        tokens[variable].clone(),
        Token::new(T_WHITESPACE, " "),
        Token::new(T_COALESCE, "??"),
        Token::new(T_WHITESPACE, " "),
    ];
    if !checked_is_then {
        items.extend((first..=last).map(|i| tokens[i].clone()));
    }
    tokens.override_range(start, end, items);
}

/// The `:` of the ternary whose `?` is at `question`, when its middle operand holds no operator that binds looser
/// than `??` (a nested ternary, `and`, `or`, `xor`), so it can become the right operand of `??`.
fn ternary_colon(tokens: &Tokens, question: usize) -> Option<usize> {
    let mut index = question + 1;
    while index < tokens.len() {
        let token = &tokens[index];
        if let Some((block, true)) = Tokens::detect_block_type(token) {
            index = tokens.find_block_end(block, index);
        } else if token.equals(":", true) {
            return Some(index);
        } else if token.equals_any(&["?".into(), ";".into(), ")".into(), "]".into(), "}".into(), ",".into()], true)
            || token.is_given_kind(&[
                T_LOGICAL_AND,
                T_LOGICAL_OR,
                T_LOGICAL_XOR,
                T_CLOSE_TAG,
                CT_ARRAY_BRACKET_CLOSE,
            ])
        {
            return None;
        }
        index += 1;
    }
    None
}

/// Whether the token before a condition binds looser than a ternary, so the condition is all of the ternary's.
fn is_boundary(token: &Token) -> bool {
    token.equals_any(&["(".into(), ",".into(), ";".into(), "[".into(), "{".into(), "}".into(), "=".into()], true)
        || token.is_given_kind(&[
            CT_ARRAY_BRACKET_OPEN,
            T_RETURN,
            T_ECHO,
            T_PRINT,
            T_YIELD,
            T_DOUBLE_ARROW,
            T_OPEN_TAG,
            T_OPEN_TAG_WITH_ECHO,
            T_LOGICAL_AND,
            T_LOGICAL_OR,
            T_LOGICAL_XOR,
            T_PLUS_EQUAL,
            T_MINUS_EQUAL,
            T_MUL_EQUAL,
            T_DIV_EQUAL,
            T_CONCAT_EQUAL,
            T_MOD_EQUAL,
            T_AND_EQUAL,
            T_OR_EQUAL,
            T_XOR_EQUAL,
            T_SL_EQUAL,
            T_SR_EQUAL,
            T_POW_EQUAL,
            T_COALESCE_EQUAL,
        ])
}

fn fix_isset(tokens: &mut Tokens, index: usize) {
    let prev_token_index = tokens.prev_meaningful(index);
    if is_higher_precedence_associativity_operator(&tokens[prev_token_index]) {
        return;
    }
    let start_brace_index = tokens
        .get_next_token_of_kind(index, &["(".into()], true)
        .unwrap();
    let end_brace_index = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, start_brace_index);
    let ternary_question_mark_index = tokens.next_meaningful(end_brace_index);
    if !tokens[ternary_question_mark_index].equals("?", true) {
        return;
    }
    let isset_tokens = meaningful_sequence(tokens, start_brace_index, end_brace_index);
    if has_changing_content(&isset_tokens) {
        return;
    }
    let isset_code = code(&isset_tokens);
    if isset_code == b"$this" {
        return;
    }
    let ternary_colon_index = tokens
        .get_next_token_of_kind(ternary_question_mark_index, &[":".into()], true)
        .unwrap();
    let first_operand =
        meaningful_sequence(tokens, ternary_question_mark_index, ternary_colon_index);
    if isset_code != code(&first_operand) {
        return;
    }
    let ternary_first_operand_index = tokens.next_meaningful(ternary_question_mark_index);
    let mut comments = Vec::new();
    let mut comment_started = false;
    for loop_index in index..ternary_first_operand_index {
        if tokens[loop_index].is_comment() {
            comments.push(tokens[loop_index].clone());
            comment_started = true;
        } else if comment_started {
            if tokens[loop_index].is_whitespace(None) {
                comments.push(tokens[loop_index].clone());
            }
            comment_started = false;
        }
    }
    tokens.set(ternary_colon_index, Token::new(T_COALESCE, "??"));
    tokens.override_range(index, ternary_first_operand_index - 1, comments);
}

fn meaningful_sequence(tokens: &Tokens, start: usize, end: usize) -> Vec<Token> {
    let mut sequence = Vec::new();
    let mut index = start;
    while let Some(next) = tokens.get_next_meaningful_token(index) {
        if next >= end {
            break;
        }
        sequence.push(tokens[next].clone());
        index = next;
    }
    sequence
}

fn code(tokens: &[Token]) -> Vec<u8> {
    tokens
        .iter()
        .flat_map(|t| t.get_content().iter().copied())
        .collect()
}

fn is_higher_precedence_associativity_operator(token: &Token) -> bool {
    token.is_given_kind(&[
        T_ARRAY_CAST,
        T_BOOLEAN_AND,
        T_BOOLEAN_OR,
        T_BOOL_CAST,
        T_COALESCE,
        T_DEC,
        T_DOUBLE_CAST,
        T_INC,
        T_INT_CAST,
        T_IS_EQUAL,
        T_IS_GREATER_OR_EQUAL,
        T_IS_IDENTICAL,
        T_IS_NOT_EQUAL,
        T_IS_NOT_IDENTICAL,
        T_IS_SMALLER_OR_EQUAL,
        T_OBJECT_CAST,
        T_POW,
        T_SL,
        T_SPACESHIP,
        T_SR,
        T_STRING_CAST,
        T_UNSET_CAST,
    ]) || token.equals_any(
        &[
            "!".into(),
            "%".into(),
            "&".into(),
            "*".into(),
            "+".into(),
            "-".into(),
            "/".into(),
            ":".into(),
            "^".into(),
            "|".into(),
            "~".into(),
            ".".into(),
        ],
        true,
    )
}

fn has_changing_content(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .any(|t| t.is_given_kind(&[T_DEC, T_INC, T_YIELD, T_YIELD_FROM]) || t.equals("(", true))
}

#[cfg(test)]
mod tests {
    fn fix(options: &serde_json::Value, code: &str) -> String {
        let mut fixer = super::fixer();
        fixer.configure(options).unwrap();
        crate::fix_with(&*fixer, code)
    }

    /// The expectation is what Rector 2.8.4's `TernaryToNullCoalescingRector` writes for the same input. Pun leaves the
    /// property and the offset that Rector also rewrites (`$o->p ?? 'x'`), since `??` calls `__isset()` or
    /// `offsetExists()` where the comparison read the value, and the rest Rector leaves too.
    #[test]
    fn null_checks_rewrite_what_rector_rewrites() {
        let before = "<?php
$x = $a !== null ? $a : 'x';
$x = null !== $a ? $a : 'x';
$x = $a === null ? 'x' : $a;
$x = NULL === $a ? 'x' : $a;
$x = is_null($a) ? 'x' : $a;
$x = !is_null($a) ? $a : 'x';
$x = \\is_null($a) ? 'x' : $a;
foo($a === null ? $b . 'c' : $a, $a !== null ? $a : $b ?? $c);
return $a !== null
    ? $a
    : 'x';
$x = $a != null ? $a : 'x';
$x = $a !== null ? $b : 'x';
$x = $a === null ? 'x' : $a . 'y';
$x = $o->p !== null ? $o->p : 'x';
$x = $a['k'] === null ? 'x' : $a['k'];
$x = $b && $a !== null ? $a : 'x';
$x = $a === null ? ($c ? 1 : 2) : $a;
";
        let after = "<?php
$x = $a ?? 'x';
$x = $a ?? 'x';
$x = $a ?? 'x';
$x = $a ?? 'x';
$x = $a ?? 'x';
$x = $a ?? 'x';
$x = $a ?? 'x';
foo($a ?? $b . 'c', $a ?? $b ?? $c);
return $a ?? 'x';
$x = $a != null ? $a : 'x';
$x = $a !== null ? $b : 'x';
$x = $a === null ? 'x' : $a . 'y';
$x = $o->p !== null ? $o->p : 'x';
$x = $a['k'] === null ? 'x' : $a['k'];
$x = $b && $a !== null ? $a : 'x';
$x = $a ?? ($c ? 1 : 2);
";
        assert_eq!(fix(&serde_json::json!({ "null_checks": true }), before), after);
        assert_eq!(fix(&serde_json::Value::Null, before), before);
    }

    /// Forms whose rewrite would change the result or the parse, which Pun leaves: a middle operand with `and` or a
    /// nested ternary (`$a ?? $b and $c` is `($a ?? $b) and $c`), a comment, `?:`, and an `is_null` that is a method.
    #[test]
    fn null_checks_leave_what_would_change_meaning() {
        let code = "<?php
$x = $a === null ? $b and $c : $a;
$x = $a === null ? $c ? 1 : 2 : $a;
$x = $a !== null /* set */ ? $a : 'x';
$x = $a !== null ?: 'x';
$x = $o->is_null($a) ? 'x' : $a;
$x = is_null($a, $b) ? 'x' : $a;
$x = $a === null ? 'x' : $b;
$x = $a === $b ? 'x' : $a;
$x = $a === null ? 'x' : $a;
";
        let fixed = "<?php
$x = $a === null ? $b and $c : $a;
$x = $a === null ? $c ? 1 : 2 : $a;
$x = $a !== null /* set */ ? $a : 'x';
$x = $a !== null ?: 'x';
$x = $o->is_null($a) ? 'x' : $a;
$x = is_null($a, $b) ? 'x' : $a;
$x = $a === null ? 'x' : $b;
$x = $a === $b ? 'x' : $a;
$x = $a ?? 'x';
";
        assert_eq!(fix(&serde_json::json!({ "null_checks": true }), code), fixed);
    }

    #[test]
    fn options() {
        let mut fixer = super::fixer();
        for (options, error) in [
            (serde_json::json!({ "foo": true }), "[ternary_to_null_coalescing] Is not configurable."),
            (
                serde_json::json!({ "null_checks": 1 }),
                "[ternary_to_null_coalescing] null_checks must be a bool, not 1",
            ),
            (serde_json::json!(1), "[ternary_to_null_coalescing] Is not configurable."),
        ] {
            assert_eq!(fixer.configure(&options).unwrap_err().to_string(), error);
        }
    }
}
