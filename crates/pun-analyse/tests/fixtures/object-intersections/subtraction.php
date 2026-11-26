<?php
interface A {}
interface B {}
interface C { public function ready(): bool; }
function merged($query, bool $flag): void {
    if ($query instanceof A) {
        return;
    }
    \PHPStan\dumpType($query);
    $selected = $query instanceof C && $flag;
    \PHPStan\dumpType($query);
}
function narrowing($query, bool $flag): void {
    if ($query instanceof A) {
        return;
    }
    if ($query instanceof C) {
        \PHPStan\dumpType($query);
        \PHPStan\dumpType($query->ready());
        if ($query instanceof A) {
            \PHPStan\dumpType($query);
        } else {
            \PHPStan\dumpType($query);
        }
        $copy = $query;
        $value = $flag ? $copy : $query;
        \PHPStan\dumpType($value);
    }
}
function scalar($query): void {
    if (is_int($query) || (is_string($query) && $query !== '')) {
        \PHPStan\dumpType($query);
    } else {
        \PHPStan\dumpType($query);
    }
}
abstract class File {}
abstract class Document extends File {}
interface Storable { public function content(): string; }
final class Remote extends Document implements Storable { public function content(): string { return ''; } }
final class Local extends Document implements Storable { public function content(): string { return ''; } }
final class Upload {}
function child(File $file): void {
    if ($file instanceof Remote) {
        return;
    }
    \PHPStan\dumpType($file);
    \PHPStan\dumpType($file instanceof Document);
    \PHPStan\dumpType($file instanceof Remote);
    if ($file instanceof Document) {
        \PHPStan\dumpType($file);
    }
}
function childAndInterface(File $file): void {
    if ($file instanceof Remote) {
        return;
    }
    if ($file instanceof Document && $file instanceof Storable) {
        \PHPStan\dumpType($file);
        \PHPStan\dumpType($file->content());
    }
}
function excluded(File $file): void {
    if ($file instanceof Remote) {
        return;
    }
    if ($file instanceof Remote) {
        \PHPStan\dumpType($file);
    }
}
function excludedUnion($file): string {
    if (! $file instanceof File && ! $file instanceof Upload) {
        return '';
    }
    return match (true) {
        $file instanceof Remote => 'remote',
        $file instanceof Local => 'local',
        $file instanceof Document && $file instanceof Storable => $file->content(),
        default => '',
    };
}
function excludedArm(File|Upload $file): string {
    return match (true) {
        $file instanceof Remote => 'remote',
        $file instanceof Document && $file instanceof Storable => [\PHPStan\dumpType($file), $file->content()][1],
        default => '',
    };
}
function pair(File $file, Document $document): void {
    if ($file instanceof Remote || $document instanceof Local) {
        return;
    }
    if ($file === $document) {
        \PHPStan\dumpType($file);
        \PHPStan\dumpType($document);
    }
}
