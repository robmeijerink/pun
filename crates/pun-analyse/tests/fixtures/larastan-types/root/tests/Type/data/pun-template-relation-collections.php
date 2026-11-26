<?php

namespace PunTemplateRelationCollections;

use App\User;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;

use function PHPStan\Testing\assertType;

final class ChildRows
{
    /**
     * @template TRow of Model
     *
     * @param  HasMany<TRow, User>  $relation
     * @param  list<int>  $keep
     */
    public static function prune(HasMany $relation, array $keep): void
    {
        $existing = $relation->get();
        assertType('Illuminate\Database\Eloquent\Collection<int, TRow of Illuminate\Database\Eloquent\Model (method PunTemplateRelationCollections\ChildRows::prune(), argument)>', $existing);

        $existing->each(fn (Model $row) => $row->delete());
    }
}
