<?php

declare(strict_types=1);

final class ForeachKey
{
    public function lengths(string $id): array
    {
        $totals = [];
        $totals[$id] = 1;
        $out = [];
        foreach ($totals as $key => $total) {
            $out[] = strlen((string) $key);
        }

        return $out;
    }
}
