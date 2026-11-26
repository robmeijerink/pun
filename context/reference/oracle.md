# Oracle

The pinned upstream tools every Pun change is judged against. `scripts/oracle.sh` installs and verifies them in
`.oracle/` (gitignored); `scripts/corpus.sh` checks the corpus out into `corpus/`.

## Pinned versions

| component | version | ref |
|---|---|---|
| PHP | 8.5.12 (system, NTS) | |
| Composer | 2.11.2 (system; also the package manager's oracle, plan/packages.md) | |
| laravel/pint | v1.32.1 | `006bd7825eb01daf92959d060b89cd9514fe4253` |
| friendsofphp/php-cs-fixer | v3.95.25 (bundled in Pint's PHAR and its composer.lock) | `2cdfc1f3daf173d83a1ebb6177949b58c95de6ff` |
| rector/rector | 2.9.2 (`.oracle/rector-bin`, locked by `scripts/oracle-rector/composer.lock`) | |
| phpstan/phpstan for Rector | 2.3.5 (both Rector oracle locks, pinned apart from the analyse oracle's PHPStan) | |
| rectorphp/rector-src | 2.9.2 (source and `rules-tests/` fixtures in `.oracle/rector`) | `e8210558743a0d5fc3e2fc32c385b78833765822` |
| rector/rector-phpunit | dev-main@d0b6caa (bundled in rector/rector 2.9.2) | |
| driftingly/rector-laravel | 2.7.3 (`.oracle/rector-ecosystem`, locked by `scripts/oracle-rector-ecosystem/composer.lock`; tests and stubs at `699f25b`) | |
| pestphp/pest-plugin-rector | 5.x-dev (`.oracle/rector-ecosystem`, same lock, `minimum-stability: dev` with `prefer-stable`) | `8c1b780c5e551cb78150ee91ceb73d34cd029946` |
| prettier / prettier-plugin-blade / prettier-plugin-tailwindcss | 3.10.1 / 3.3.3 / 0.8.1 (only for `Pint/laravel_blade`, in `corpus/pinkary/node_modules`) | |
| Node | v26.12.1 (system; scripts/oracle.sh does not pin it) | |

- Pint: `php .oracle/pint/builds/pint` (the PHAR committed at the tag). `.oracle/pint` is the source with dev deps,
  which `scripts/pint-boot.php` uses to resolve configs.
- PHP-CS-Fixer: `php .oracle/php-cs-fixer/php-cs-fixer`, a source checkout with `tests/` and PHPUnit at the bundled ref.
- Rector: `php .oracle/rector-bin/vendor/bin/rector process --config <rector.php>`. Rule fixtures are in
  `.oracle/rector/rules-tests/`.
- Pint registers three custom fixers: `Pint/laravel_blade` (Prettier via Node, or Bun in a project with a Bun lock
  file), `Pint/phpdoc_type_annotations_only` (priority 56, no preset enables it) and `Pint/prettier_cache_fingerprint`
  (a no-op for the cache signature; Pun has no port and nothing enables it). Pint also makes `no_unused_imports`,
  `fully_qualified_strict_types`, `global_namespace_import` and `declare_strict_types` skip `*.blade.php` files
  (Pun: `BLADE_UNSAFE` in `pun-format`).

## Scripts

| script | what |
|---|---|
| `scripts/oracle.sh` | install/verify `.oracle/` (checks the PHAR bundles the pinned php-cs-fixer ref) |
| `scripts/corpus.sh [name...]` | shallow checkouts per `corpus/manifest.toml`; builds derived `pinkary-dirty` |
| `scripts/effective-rules.php [pint.json]` | effective fixer list (run order, options, priority, risky, class, file) |
| `scripts/export-fixtures.php <effective.json> <outdir>` | upstream data-provider test cases to JSON |
| `scripts/cli-parity.sh [--project DIR] [--tty [--cols N]] [--env K=V] "ARGS"...` | compare Pint and `pun migrate` + `pun format` per argument set, in the same directory: exit code and files, and stdout/stderr byte for byte for machine formats |
| `scripts/export-phpunit-stub.php` | PHPUnit's declarations for `crates/pun-lint/tests/phpunit-stubs` |
| `scripts/gen-third-party-licenses.sh [--check]` | `licenses/THIRD_PARTY_LICENSES.md` from cargo-about |
| `scripts/perturb.php <dir>` | make a deliberately unformatted copy (anti-laravel config + token noise, `php -l` checked) |
| `php scripts/fixer-metadata.php --gitlab > crates/pun-cli/src/gitlab_fixers.json` | the gitlab reporter's fixer descriptions and docs paths |

## Rector

The lint oracle (lint.md §6). A Pun lint rule ports one Rector rule. Its tests are that rule's own fixtures from
`.oracle/rector/rules-tests/`, copied verbatim into `crates/pun-lint/tests/rector/<Rule>/`. A fixture is the input, a
`-----` line and the expected output, or only the input when the rule must leave it alone; the rule's
`config/configured_rule.php` gives the PHP version. Parity is `format(pun lint --fix) == format(expected)`. Use the
Rector oracle's PHPStan, not the analyse oracle's, for lint expectations: their types can differ.

For an input the fixtures miss, run Rector on it and paste its output as the expectation
(`crates/pun-lint/tests/oracle/`):

```php
<?php // rector.php next to a src/ holding the inputs
use Rector\Config\RectorConfig;
return RectorConfig::configure()
    ->withPaths([__DIR__.'/src'])
    ->withRules([\Rector\CodeQuality\Rector\Assign\CombinedAssignRector::class])
    ->withPhpVersion(80400);
```

`php .oracle/rector-bin/vendor/bin/rector process --no-progress-bar --no-diffs --clear-cache --config rector.php`

Two traps make the output look like an upstream change when it is not:

- A case that uses PHPUnit needs PHPUnit's classes loaded before Rector loads its own stub `TestCase`. Register an
  autoloader for `tests/phpunit-stubs` in a bootstrap file passed through `->withBootstrapFiles([...])`. Rector reads
  package versions (PHPUnit 12.5, Laravel 13) from `vendor/composer/installed.json` next to the config.
- Autoload the classes the test names with `->withAutoloadPaths([...])`: the rule's `Source/`, and `Stubs/` where the
  test directory has one. Without them Rector resolves no annotation class and leaves Doctrine annotations alone.

## Harness

```
cargo build -p punphp-harness
target/debug/pun-harness format                         # every corpus project, regenerates context/reports/scoreboard.md
target/debug/pun-harness format --corpus pinkary        # one project (repeatable)
target/debug/pun-harness format --fixer single_quote --corpus pinkary-dirty --diffs 5
```

Per project, the harness copies `corpus/<name>` twice into a temp dir (no `.git`, `node_modules` symlinked). It runs
Pint in one copy and `pun migrate` then `pun check --fix` in the other, so every run also tests the migration. It
byte-compares every file except pun.toml, prints counts and the first `--diffs` unified diffs (`--- oracle` /
`+++ pun`), and exits 1 on any differing file.

`--fixer <name>` writes a pint.json with the project's finder keys, `"preset": "empty"` and only that rule, with the
options from the project's effective rule list (`true` if the project doesn't enable it). It passes that file as
`--config` to Pint and to `pun migrate`, and leaves the scoreboard alone.

## Fixtures

`crates/pun-fixers/tests/fixtures/<fixer>.json` holds, per test method with a data provider: its params
(name/type/default), its required PHP version, `declared_in`, the PHP body (to see how args are used) and every case
`{provider, key, args}`. Objects are dumped as `{"__class": ..., props}`, non-UTF-8 strings as `{"__base64": ...}`.
Non-fix providers (invalid-configuration cases) are included; filter on `method`.

## CLI output parity

The human output is Pun's own (plan/output.md); `crates/pun-cli/tests/output.rs` checks it against hand-derived
samples, with and without colour, piped and on a terminal. `scripts/cli-parity.sh` and `output.rs` compare with the
PHAR what stays Pint's: exit codes, resulting files, `-q`, every `--format=` reporter (json/txt/agent/checkstyle/
junit/xml/gitlab, stdout and `--output-to-file`), abort boxes and the prettier pre-flight check.

Syntax-error messages are PHP's own without running PHP: `pun-format`'s PHP grammar (`php_grammar.rs`, generated by
`scripts/gen-php-grammar.py`) fails with the messages `token_get_all(..., TOKEN_PARSE)` gives.

Intentional differences:

- `-vvv`: Pint's PHAR wrapper prints a "Box Requirements Checker" block on stderr. It is not Pint output, so Pun
  doesn't print it (cli-parity.sh strips it).
- Missing prettier packages for `Pint/laravel_blade`: interactive Pint asks whether to install them. Pun always takes
  the non-interactive answer (no) and aborts with Pint's message.
- Cache: Pint writes PHP-CS-Fixer's cache file, which records the PHP runtime's version. Pun keeps its own result
  cache in `$PUN_CACHE_DIR/results`, outside the project. The output is the same, because a dry run caches no file
  that needs fixing.
- Errors other than Pint's `abort()` (unreadable path, bad config) get a Symfony "In File.php line N:" box naming PHP
  source in Pint. Pun prints the message without that box.

## PHPStan (crate `pun-phpstan`)

| component | version | ref |
|---|---|---|
| phpstan/phpstan | 2.3.5 (Pun's parity target; PHAR, vendor prefixed `_PHPStan_95d9e8b05\`) | phpstan-src `95d9e8b0578ebf75439a3744a845ef25417028fa` |
| larastan/larastan | v3.14.0 | |
| phpstan/phpstan-deprecation-rules | ^2.0 (Pinkary requires it) | |
| nette/neon | v3.3.4 (bundled in the PHAR) | `bb88bf3a54dd21bf4dbddb5cd525d7b0c61b7cda` |
| nette/di / nette/schema | 3.1.10 / 1.2.5 (bundled in the PHAR) | |

- `.oracle/phpstan`: `composer install` from the committed `scripts/oracle-phpstan/composer.{json,lock}`. Binary:
  `php .oracle/phpstan/vendor/phpstan/phpstan/phpstan.phar`. Its `vendor/` also serves the corpus parity tests
  (symlinked into a farm, so `%rootDir%` and `includes: vendor/...` resolve as in a real project).
- `.oracle/neon`: nette/neon source and tests at v3.3.4. `scripts/oracle.sh` does not fetch it.
- `.oracle/phpstan-src`: phpstan-src at 2.3.5; its tests give data providers and its rules the PHPStan inventory.
- `.oracle/phpstan-phar`: an extracted copy of the PHAR, for reading only.
- The rule mapping's inventory read phpstan/phpstan-strict-rules 2.0.12 and phpstan/phpstan-deprecation-rules 2.0.5
  (`crates/pun-config/rules/upstream/phpstan-extensions.json`).

Every exporter runs the PHAR's real code, so expected values are never written by hand. Re-run one after touching its
cases, then the tests.

| script | writes (under `crates/pun-phpstan/tests/fixtures/`) |
|---|---|
| `php scripts/export-neon-fixtures.php .oracle/neon > neon.json` | every Neon decode/encode nette/neon's tests make; with `neon-extra-inputs.json` as 2nd arg, decodes those strings (`neon-extra.json`) |
| `php scripts/phpstan-config.php <phar> <cwd> [-c f] [-l n] [--generated-config f] [--generate-baseline f] [path...]` | (stdout) resolved parameters and allConfigFiles, or PHPStan's stderr error, as `phpstan analyse` gets them |
| `php scripts/export-phpstan-config-fixtures.php [case...]` | `config/<case>/expected.json` from `case.json`, and `config-corpus/<name>.json` (pinkary, larastan, laravel-pint, laravel-framework-src/-types) |
| `php scripts/export-schema-fixtures.php` | `schema.json`: parameter schema validation |
| `php scripts/export-ignore-fixtures.php` | `comments.json` (ignore comments), `local-processor.json` (local ignores), `ignore.json` (`ignoreErrors` matching and result processing) |
| `php scripts/export-local-ignore-fixtures.php` | `local.json`: real `phpstan analyse` on `local/*.php`, comments blanked vs. kept |
| `php scripts/export-php-helpers-fixtures.php` | `php.json`: regex matching, preg_quote, file helpers, relative paths, fnmatch, (string) casts, `phpfile/ok-*.php` |
| `php scripts/export-phpstan-symbols.php` | `../data/symbols.json` (every built-in function, method, class and constant as PHPStan 2.3.5 reflects them on PHP 8.4, with the 8.5 differences), `type-strings.json` (every signature type string, resolved and described), `reflected-types.json` (every type in those signatures, as PHPDoc and described) |
| `php scripts/export-phpstan-symbols.php probes` | `type-probes.json` (hand-picked and generated type strings: descriptions and iterable key/value), `type-super.json` (`isSuperTypeOf()` of every ordered pair within each atom set) |

Pun does not re-implement how PHPStan builds a built-in's signature from its stubs and function maps. The exporter
asks PHPStan's reflection for every built-in and stores the result in `crates/pun-phpstan/data/symbols.json`.
`pun-phpstan`'s type parser (`phpdoc.rs`), resolver (`resolve.rs`) and type model (`types.rs`) read it back and are
tested against the fixtures above.

### `pun analyse` (crate `pun-analyse`)

Rule fixtures and corpus parity run the real PHAR at level 0. The scripts handle two environment traps:

- PHPStan detects AI agents (`AI_AGENT`, `CLAUDECODE`, `AMP_CURRENT_THREAD_ID`, ...) and switches its output format.
  The scripts unset those variables.
- PHP 8.5's `max_memory_limit` (512M in php.ini) caps `--memory-limit`, also in worker processes, which don't inherit
  `-d`. The scripts set `PHP_INI_SCAN_DIR=:target-analyse/php-ini` with `max_memory_limit = -1` there.

| script | writes |
|---|---|
| `php scripts/export-rule-fixtures.php [--resume] [--budget=SECONDS]` | `crates/pun-analyse/tests/fixtures/rules/<Dir>/<Test>.json`: every `analyse()` call of every level-0 rule test (data providers expanded, PHP version requirements applied), re-run as `phpstan analyse --level 0` on the same files, plus copies of the data files under `data/` |
| `php scripts/export-phpstan-api-stub.php > crates/pun-phpstan/data/phpstan-api.php` | the PHAR's unprefixed classes (PHPStan, PhpParser with its compatibility aliases, phpdoc-parser, BetterReflection), bodies stripped; Pun scans it when an analysed file names those namespaces |
| `python3 scripts/analyse-corpus.py setup\|oracle\|diff\|rank <project>` | corpus checkouts under `target-analyse/corpus` (larastan at tag v3.14.0), PHPStan's level-0 JSON in `target-analyse/oracle/<p>.json` (cold, one worker), and the per-identifier diff against `examples/analyse` |

`tests/rules.rs` compares only the identifiers `pun_analyse::IDENTIFIERS` lists. Cases that still differ are listed,
with reasons, in `tests/fixtures/rules/gaps.txt`, and the list must be exact.

Mago provides parsing, name resolution, the codex (with its prelude for built-ins) and the final type of each
expression; it has no scope snapshot per statement. Where it falls short:

- Its user-defined flag is set only for host files, so vendored symbols need `!is_built_in()`.
- It inherits constructors only from "consistent" parents and drops private parent methods. Pun's `rules/members.rs`
  does its own method lookup (own, traits with `as` aliases, parents, interfaces; built-in classes from `symbols.json`).
- `@consistent-constructor` and a final constructor set the same flag.
- An object of an unknown class is typed `mixed`, so `new $object` on it can't report the missing class.
- It has no class aliases, so the API stub declares them as subclasses. Laravel's aliases come from Pun's own
  statically resolved registry ([analyse-behaviour.md](analyse-behaviour.md)).
- Every rule decision about built-ins uses `symbols.json`, not Mago's prelude.

## The rule mapping's inventories

`crates/pun-config/rules/upstream/{rector-core,rector-ecosystem,phpstan-core,phpstan-extensions}.json` list every
upstream rule at the pinned versions: its class, package, sets (Rector) or identifiers and level (PHPStan), whether a
planned set registers it (`in_scope`, with `out_of_scope_reason`), and whether it is deprecated. Larastan 3.14's new
rules are not in `phpstan-extensions.json` yet (../PLAN.md §3 Foundations).
`scripts/check-rule-mapping.py` checks mapping.toml against them (rules.md §5, §7). Rector core and rector-phpunit
come from `.oracle/rector-bin`, rector-laravel and Pest from `.oracle/rector-ecosystem`, PHPStan from
`.oracle/phpstan` and `.oracle/phpstan-src`.
