<?php

namespace MixedArrayGuards;

function positiveGuard(mixed $value, bool $choose): void
{
    if (is_array($value)) {
        \PHPStan\dumpType($value);
        $copy = $value;
        \PHPStan\dumpType($copy);
        if ($choose) {
            $copy = [];
        }
        \PHPStan\dumpType($copy);
        if ($choose) {
            $copy = [5];
        }
        \PHPStan\dumpType($copy);
        if (is_array($copy)) {
            \PHPStan\dumpType($copy);
        }
    }
}

function implicitArray(array $value): void
{
    \PHPStan\dumpType($value);
    $copy = $value;
    if (is_array($copy)) {
        \PHPStan\dumpType($copy);
    }
}

function implicitUnknown($value): void
{
    \PHPStan\dumpType($value);
    if (is_array($value)) {
        \PHPStan\dumpType($value);
    }
}

function falseyIntersections(mixed $value): void
{
    if (!$value) {
        if (is_int($value)) {
            \PHPStan\dumpType($value);
        }
        if (is_float($value)) {
            \PHPStan\dumpType($value);
        }
        if (is_string($value)) {
            \PHPStan\dumpType($value);
        }
        if (is_array($value)) {
            \PHPStan\dumpType($value);
        }
        if (is_bool($value)) {
            \PHPStan\dumpType($value);
        }
        if (is_null($value)) {
            \PHPStan\dumpType($value);
        }
    }
}

function copiedExclusion(mixed $value, bool $choose): void
{
    if (!is_array($value)) {
        $copy = $value;
        \PHPStan\dumpType($copy);
        if ($choose) {
            unset($copy['removed']);
        }
        \PHPStan\dumpType($copy);
        if (!is_string($copy) && !$copy instanceof \ArrayAccess) {
            \PHPStan\dumpType($copy);
            unset($copy['possible']);
        }
    }
}

/** @param array<string, int>|null $value */
function typedArrayGuard(?array $value): void
{
    if (is_array($value)) {
        \PHPStan\dumpType($value);
    }
}

/** @param array<array-key, int> $value */
function keyAliasGuard(array $value): void
{
    if (is_array($value)) {
        \PHPStan\dumpType($value);
    }
}

/** @param array<int|string, int> $value */
function keyUnionGuard(array $value): void
{
    if (is_array($value)) {
        \PHPStan\dumpType($value);
    }
}

namespace CustomArrayGuard;

/**
 * @phpstan-assert-if-true string $value
 * @assert-if-true string $value
 */
function is_array(mixed $value): bool
{
    return \is_string($value);
}

function customGuard(mixed $value): void
{
    if (is_array($value)) {
        \PHPStan\dumpType($value);
    }
    if (\is_array($value)) {
        \PHPStan\dumpType($value);
    }
}

namespace UnspecifiedCustomArrayGuard;

function is_array(mixed $value): bool
{
    return \is_string($value);
}

function customGuard(mixed $value): void
{
    if (is_array($value)) {
        \PHPStan\dumpType($value);
    }
}

namespace MixedAccessoryJoins;

class Excluded {}

class Ranked {}

/**
 * @param non-empty-array<mixed> $nonEmpty
 * @param list<int> $list
 * @param array{int, string} $tuple
 * @param array{a?: int} $optional
 * @param array<int> $plain
 * @param non-empty-string $nonEmptyString
 * @param numeric-string $numericString
 * @param non-falsy-string $nonFalsy
 * @param lowercase-string $lowercase
 * @param literal-string $literal
 * @param callable-string $callableString
 * @param numeric $numeric
 */
function joins(
    mixed $value,
    bool $choose,
    array $nonEmpty,
    array $list,
    array $tuple,
    array $optional,
    array $plain,
    string $nonEmptyString,
    string $numericString,
    string $nonFalsy,
    string $lowercase,
    string $literal,
    string $callableString,
    $numeric,
    string $string,
): void {
    if ($value instanceof Excluded) {
        return;
    }
    if ($choose) { $x = $value; } else { $x = $nonEmpty; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $list; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $tuple; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $optional; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $plain; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } elseif ($plain !== []) { $x = $plain; } else { $x = $nonEmpty; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $nonEmptyString; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $numericString; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $nonFalsy; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $lowercase; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $literal; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $callableString; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $numeric; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = $string; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = 'literal'; }
    \PHPStan\dumpType($x);
    if ($choose) { $x = $value; } else { $x = Ranked::class; }
    \PHPStan\dumpType($x);
}

function offsets(mixed $value): void
{
    if ($value instanceof Excluded) {
        return;
    }
    if (is_array($value) && isset($value[0]) && $value[0] instanceof Ranked) {
        return;
    }
    \PHPStan\dumpType($value);
}

function guards(mixed $value): void
{
    if ($value instanceof Excluded) {
        return;
    }
    if (is_array($value) && isset($value[0])) {
        return;
    }
    \PHPStan\dumpType($value);
}
