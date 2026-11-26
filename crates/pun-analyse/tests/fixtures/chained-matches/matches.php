<?php

declare(strict_types=1);

namespace ChainedMatches;

final class Morph
{
    public function getMorphClass(): string
    {
        return static::class;
    }
}

final class MatchKind
{
    public static function label(string $type): string
    {
        $kind = match ($type) {
            (new Morph())->getMorphClass() => 'support',
            'span' => 'span',
            default => 'whole',
        };

        return match ($kind) {
            'support' => 'Support',
            'span' => 'Span',
            default => 'Whole',
        };
    }

    public static function narrowed(string $type): string
    {
        $kind = match ($type) {
            'x' => 'support',
            default => 'whole',
        };
        if ($kind === 'support') {
            return 'S';
        }

        return match ($kind) {
            'support' => 'Support',
            default => 'Whole',
        };
    }

    public static function impossible(string $type): string
    {
        $kind = match ($type) {
            'x' => 'support',
            default => 'whole',
        };

        return match ($kind) {
            'nope' => 'Nope',
            default => 'Whole',
        };
    }

    public static function nested(string $type): string
    {
        return match (match ($type) { 'x' => 'support', default => 'whole' }) {
            'support' => 'Support',
            default => 'Whole',
        };
    }
}
