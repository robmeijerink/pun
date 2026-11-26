<?php

namespace StaticArity;

/**
 * @template A
 * @template B
 */
class Base2
{
    /** @return static */
    public function copy()
    {
        return clone $this;
    }
}

/**
 * @template A
 * @template B
 * @extends Base2<A, B>
 */
class Same extends Base2
{
}

/**
 * @template A
 * @extends Base2<A, string>
 */
class Fewer extends Base2
{
}

/**
 * @template X
 * @template Y
 * @extends Base2<Y, X>
 */
class Swapped extends Base2
{
}

/**
 * @param Same<int, string> $s
 * @param Fewer<int> $f
 * @param Swapped<int, string> $w
 */
function f(Same $s, Fewer $f, Swapped $w): void
{
    \PHPStan\dumpType($s->copy());
    \PHPStan\dumpType($f->copy());
    \PHPStan\dumpType($w->copy());
}

/**
 * @template A
 * @template B
 * @template C
 */
abstract class Relation
{
    /** @return static */
    public function clone()
    {
        return clone $this;
    }
}

/**
 * @template A
 * @template B
 * @extends Relation<A, B, list<A>>
 */
class HasMany extends Relation
{
}

/** @param HasMany<int, string> $relation */
function g(HasMany $relation): void
{
    \PHPStan\dumpType($relation->clone());
    \PHPStan\dumpType($relation->clone()->clone());
}
