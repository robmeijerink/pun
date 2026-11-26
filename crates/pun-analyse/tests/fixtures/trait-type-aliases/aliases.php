<?php

declare(strict_types=1);

namespace TraitTypeAliases;

/**
 * @phpstan-type Node array{id: int, name: string}
 */
trait ReadsNodes
{
    /** @return list<int> */
    public function ids(mixed $stored): array
    {
        /** @var list<Node> $nodes */
        $nodes = $stored;

        return array_map(static fn (array $node): int => $node['id'], $nodes);
    }
}

trait ReadsMore
{
    use ReadsNodes;

    public function first(mixed $stored): string
    {
        /** @var Node $node */
        $node = $stored;

        return $node['name'];
    }
}

final class Catalog
{
    use ReadsMore;
}
