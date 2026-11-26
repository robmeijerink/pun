// `pun migrate` reads a rector.php through the project's own Rector and prints what Rector resolves as JSON: every rule
// it runs (sets included), the configured ones with their configuration and the rule class's string constants (its
// option names), withSkip() split into rules (class => null or paths) and paths, the paths, the set files, whether
// withPhpSets()/withPhpLevel() was used and the settings rector.php changes. Run as
// `php -r <this> -- <autoload> <config>` in the project root (withPhpSets() reads composer.json there). Errors print
// their message and exit 1. The JSON or the message follows the marker "\0pun-migrate\0": Rector's own warnings (a
// deprecated set) go to stdout before it.
error_reporting(E_ALL & ~E_DEPRECATED);
try {
    require $argv[1];
    $config = require $argv[2];
    if (!is_callable($config)) {
        throw new \RuntimeException('rector.php must return RectorConfig::configure() or a closure taking RectorConfig');
    }
    $rector = new \Rector\Config\RectorConfig();
    $config($rector);
    $parameter = fn (string $option): array => \Rector\Configuration\Parameter\SimpleParameterProvider::hasParameter($option)
        ? \Rector\Configuration\Parameter\SimpleParameterProvider::provideArrayParameter($option)
        : [];
    $builder = $config instanceof \Rector\Configuration\RectorConfigBuilder ? $config : null;
    $field = fn (string $name) => $builder === null ? null : (fn () => $this->$name)->call($builder);
    // Rector's SkippedClassResolver: a string key is a rule with its paths (null: everywhere); a list entry is a rule
    // when the class exists, else a path.
    $skipRules = [];
    $skipPaths = [];
    foreach ($parameter(\Rector\Configuration\Option::SKIP) as $key => $value) {
        if (is_string($key)) {
            $skipRules[$key] = $value === null ? null : array_values((array) $value);
        } elseif (is_string($value) && class_exists($value)) {
            $skipRules[$value] = null;
        } elseif (is_string($value)) {
            $skipPaths[] = $value;
        }
    }
    // A configuration as JSON: a value object is `{"@props": {property: value}}`, an enum its value or name, and what
    // JSON can't hold (a closure, a resource) `{"@unsupported": type}`.
    $export = function ($v) use (&$export) {
        if (is_array($v)) {
            return array_map($export, $v);
        }
        if ($v instanceof \UnitEnum) {
            return $v instanceof \BackedEnum ? $v->value : $v->name;
        }
        if ($v instanceof \Closure || is_resource($v)) {
            return ['@unsupported' => get_debug_type($v)];
        }
        // a PHPStan type (AddParamTypeDeclaration's, AddReturnTypeDeclaration's) as its PHPDoc string, `(string | null)`
        if ($v instanceof \PHPStan\Type\Type) {
            return (string) $v->toPhpDocNode();
        }
        if (is_object($v)) {
            $props = [];
            foreach ((new \ReflectionObject($v))->getProperties() as $p) {
                if (!$p->isStatic()) {
                    $props[$p->getName()] = $p->isInitialized($v) ? $export($p->getValue($v)) : null;
                }
            }
            return ['@props' => (object) $props];
        }
        return $v;
    };
    $configured = [];
    foreach ($rector->getRuleConfigurations() as $class => $configuration) {
        $constants = class_exists($class) ? (new \ReflectionClass($class))->getConstants() : [];
        $configured[$class] = [
            'configuration' => $export($configuration),
            'constants' => array_values(array_filter($constants, 'is_string')),
        ];
    }
    // The settings (Rector\Configuration\Option values) that change what Rector writes and that rector.php sets away
    // from Rector's defaults (its config/config.php): withImportNames(), withTreatClassesAsFinal(), withPhpVersion()...
    $parameters = \Closure::bind(static fn () => self::$parameters, null, \Rector\Configuration\Parameter\SimpleParameterProvider::class)();
    $defaults = [
        'auto_import_names' => false,
        'auto_import_doc_block_names' => false,
        'remove-unused-imports' => false,
        'treat_classes_as_final' => false,
        'php_version_features' => null,
        'file_extensions' => ['php'],
        'new_line_on_fluent_call' => false,
        'indent-char' => ' ',
        'indent-size' => 4,
        'type_guarded_classes' => [],
    ];
    $settings = [];
    foreach ($defaults as $option => $default) {
        if (array_key_exists($option, $parameters) && $parameters[$option] !== $default) {
            $settings[] = $option;
        }
    }
    echo "\0pun-migrate\0", json_encode([
        'rules' => array_values($rector->getMainRectorClasses()),
        'configured' => (object) $configured,
        'skip_rules' => (object) $skipRules,
        'skip_paths' => $skipPaths,
        'paths' => array_values($parameter(\Rector\Configuration\Option::PATHS)),
        'sets' => array_values(array_unique($field('sets') ?? [])),
        'php_sets' => (bool) ($field('isWithPhpSetsUsed') || $field('isWithPhpLevelUsed')),
        'settings' => $settings,
    ], JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR);
} catch (\Throwable $e) {
    $message = $e->getMessage();
    // rector.php calls a method the project's Rector lacks, such as withSetProviders(), which Rector 2.9.0 removed:
    // Rector itself fails on it the same way.
    if ($e instanceof \Error && str_starts_with($message, 'Call to undefined method Rector\\') && class_exists(\Rector\Application\VersionResolver::class)) {
        $message .= sprintf(' (Rector %s has no such method, so `vendor/bin/rector` fails on this rector.php too)', \Rector\Application\VersionResolver::PACKAGE_VERSION);
    }
    echo "\0pun-migrate\0", $message;
    exit(1);
}
