<?php

namespace PunNullsafeRelations;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Support\Collection;

use function PHPStan\Testing\assertType;

final class Variant extends Model
{
    /** @return HasMany<Field, $this> */
    public function fields(): HasMany
    {
        return $this->hasMany(Field::class);
    }
}

final class Field extends Model {}

final class Sheet extends Model
{
    /** @return BelongsTo<Variant, $this> */
    public function variant(): BelongsTo
    {
        return $this->belongsTo(Variant::class);
    }

    /** @return Collection<int, Field> */
    public function sortedFields(): Collection
    {
        assertType('PunNullsafeRelations\Variant|null', $this->variant);
        assertType('Illuminate\Database\Eloquent\Collection<int, PunNullsafeRelations\Field>|null', $this->variant?->fields);
        assertType('Illuminate\Database\Eloquent\Collection<int, PunNullsafeRelations\Field>|null', $this->variant?->fields->sortBy('position'));
        assertType('int<0, max>|null', $this->variant?->fields->count());

        return $this->variant?->fields->sortBy('position')->values() ?? collect();
    }
}
