#!/usr/bin/env python3
"""Write crates/pun-cli/tests/fixtures/show.json, the manifest of crates/pun-cli/tests/show.rs: small projects with what
`composer show` (`info`) 2.11.2 (.oracle/composer, the Composer the plugin host pins) prints and returns for them, and
the platform it ran on. These are the views of what is installed or locked; `--latest`, `--outdated`, `--available`
and `--all`, which ask the repositories, are left for later.

`--platform` lists the platform, so the manifest records it once, as Pun's own probe (the PROBE script of
crates/pun-packages/src/resolver/platform.rs) run on the same `php`, without its `ini` and `inputs` (local paths). The
test gives Pun that probe through PHP_BINARY, a script that prints it. So that no case reaches the network, every root
turns packagist.org off.

Each case is a set of files and the words after `composer`, run in a fresh temporary directory with a fresh HOME and
COMPOSER_HOME, COMPOSER_ROOT_VERSION=1.0.0 and COLUMNS=500 (unless the case's `env` says otherwise; a null leaves a
variable out), without a terminal on stdin. The directory is written `{dir}` in the output. A case records `exit`,
`stdout` and `stderr`, and `message` (the lines of Composer's exception block) when Composer stops with an exception,
which Pun prints as its own run error (exit 2).

usage: scripts/export-show-cases.py   (needs php 8.5 and .oracle/composer from scripts/oracle.sh)
"""
import json, os, re, shutil, subprocess, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PHAR = os.path.join(ROOT, '.oracle/composer/composer.phar')
OUT = os.path.join(ROOT, 'crates/pun-cli/tests/fixtures/show.json')
PLATFORM_RS = os.path.join(ROOT, 'crates/pun-packages/src/resolver/platform.rs')

SHA = '1234567890abcdef1234567890abcdef12345678'


def j(value):
    return json.dumps(value, indent=4) + '\n'


def package(name, version, **fields):
    normalized = re.sub(r'^v', '', version)
    if not (normalized.startswith('dev-') or normalized.endswith('-dev')):
        normalized += '.0' * (3 - normalized.count('.'))
    p = {'name': name, 'version': version, 'version_normalized': fields.pop('version_normalized', normalized), 'type': 'library'}
    p.update({k.replace('_', '-'): v for k, v in fields.items()})
    return p


def installed(*packages, dev=()):
    """vendor/composer/installed.json and each package's directory (Composer drops a package whose directory isn't
    there)."""
    files = {'vendor/composer/installed.json': j({'packages': list(packages), 'dev': True, 'dev-package-names': list(dev)})}
    for p in packages:
        if p.get('type') != 'metapackage':
            files[f'vendor/{p["name"].lower()}/.keep'] = ''
    return files


def lock(packages=(), packages_dev=(), **fields):
    return j({'content-hash': 'x', 'packages': list(packages), 'packages-dev': list(packages_dev), **fields})


ALPHA = package('acme/alpha', '1.2.3', description='The alpha package, with a description long enough to be cut on a narrow terminal',
                license=['MIT', 'Apache-2.0'], homepage='https://alpha.example', keywords=['alpha', 'first'],
                authors=[{'name': 'Ada', 'email': 'ada@example.org', 'homepage': 'https://ada.example', 'role': 'Developer'}],
                source={'type': 'git', 'url': 'https://github.com/acme/alpha.git', 'reference': SHA},
                dist={'type': 'zip', 'url': 'https://api.github.com/repos/acme/alpha/zipball/' + SHA, 'reference': SHA, 'shasum': ''},
                time='2026-01-02T03:04:05+00:00',
                support={'issues': 'https://github.com/acme/alpha/issues', 'source': 'https://github.com/acme/alpha/tree/1.2.3'},
                autoload={'psr-4': {'Acme\\Alpha\\': 'src/'}, 'files': ['src/helpers.php']},
                require={'php': '>=8.1', 'acme/gamma': '^1.0', 'ext-json': '*'}, require_dev={'phpunit/phpunit': '^11'},
                suggest={'acme/zed': 'For the zed feature'}, provide={'psr/thing-implementation': '1.0'},
                conflict={'acme/old': '<2.0'}, replace={'acme/alpha-legacy': 'self.version'},
                bin=['bin/alpha'], funding=[{'type': 'github', 'url': 'https://github.com/sponsors/ada'}])
BETA = package('acme/beta', 'dev-main', version_normalized='dev-main', description='The beta package',
               extra={'branch-alias': {'dev-main': '2.x-dev'}}, license=['BSD-3-Clause'],
               source={'type': 'git', 'url': 'https://github.com/acme/beta.git', 'reference': SHA},
               require={'acme/gamma': '^1.2'})
GAMMA = package('acme/gamma', '1.2.0', description='A transitive dependency', require={'psr/log': '^1.0 || ^2.0'})
LOG = package('psr/log', '2.0.0', description='Common interface for logging libraries', license='MIT')
TOOL = package('acme/tool', 'v3.0.0', description='A development tool', require={'psr/log': '^2.0'})
META = package('acme/meta', '1.0.0', type='metapackage', description='A metapackage', require={'psr/log': '*'})
OLD = package('acme/abandoned', '0.9.0', abandoned='acme/new', description='No longer maintained')
GONE = package('acme/gone', '1.0.0', abandoned=True)
BARE = package('Acme/Bare', '1.0.0')
PACKAGES = [GAMMA, ALPHA, BETA, LOG, TOOL, META, OLD, GONE, BARE]

NO_PACKAGIST = [{'packagist.org': False}]
ROOT_JSON = {'name': 'acme/app', 'description': 'The application', 'license': 'proprietary', 'type': 'project',
             'require': {'php': '>=8.1', 'acme/alpha': '^1.2', 'acme/beta': 'dev-main', 'acme/meta': '*', 'acme/abandoned': '*',
                         'acme/gone': '*', 'acme/bare': '*'},
             'require-dev': {'acme/tool': '^3.0'}, 'repositories': NO_PACKAGIST,
             'autoload': {'psr-4': {'App\\': 'app/'}}}


def vendor_project(manifest=None, packages=PACKAGES):
    return {'composer.json': j(ROOT_JSON if manifest is None else manifest), **installed(*packages, dev=['acme/tool'])}


def lock_project(manifest=None):
    return {'composer.json': j(ROOT_JSON if manifest is None else manifest),
            'composer.lock': lock([p for p in PACKAGES if p is not TOOL], [TOOL])}


CASES = [
    # The list of what is installed.
    ('list', vendor_project(), ['show']),
    ('info', vendor_project(), ['info']),
    ('list-json', vendor_project(), ['show', '--format=json']),
    ('list-direct', vendor_project(), ['show', '-D']),
    ('list-direct-json', vendor_project(), ['show', '--direct', '-f', 'json']),
    ('list-no-dev', vendor_project(), ['show', '--no-dev']),
    ('list-name-only', vendor_project(), ['show', '-N']),
    ('list-name-only-json', vendor_project(), ['show', '--name-only', '--format=json']),
    ('list-path', vendor_project(), ['show', '-P']),
    ('list-path-json', vendor_project(), ['show', '--path', '-f', 'json']),
    ('list-installed-option', vendor_project(), ['show', '-i']),
    ('list-ansi', vendor_project(), ['show', '--ansi']),
    # Narrow terminals cut the descriptions.
    ('list-narrow', vendor_project(), ['show'], {'COLUMNS': '60'}),
    ('list-very-narrow', vendor_project(), ['show'], {'COLUMNS': '30'}),
    ('list-no-columns', vendor_project(), ['show'], {'COLUMNS': None}),
    # Wildcards.
    ('pattern', vendor_project(), ['show', 'acme/*']),
    ('pattern-case', vendor_project(), ['show', 'ACME/A*', '-N']),
    ('pattern-none', vendor_project(), ['show', 'nope/*']),
    # The tree.
    ('tree', vendor_project(), ['show', '--tree']),
    ('tree-json', vendor_project(), ['show', '-t', '--format=json']),
    ('tree-package', vendor_project(), ['show', '-t', 'acme/alpha']),
    ('tree-package-json', vendor_project(), ['show', '-t', 'acme/beta', '-f', 'json']),
    ('tree-ansi', vendor_project(), ['show', '--tree', '--ansi']),
    ('tree-path', vendor_project(), ['show', '--tree', '--path']),
    # One package.
    ('package', vendor_project(), ['show', 'acme/alpha']),
    ('package-json', vendor_project(), ['show', 'acme/alpha', '--format=json']),
    ('package-ansi', vendor_project(), ['show', 'acme/alpha', '--ansi']),
    ('package-alias', vendor_project(), ['show', 'acme/beta']),
    ('package-alias-json', vendor_project(), ['show', 'acme/beta', '-f', 'json']),
    ('package-bare', vendor_project(), ['show', 'acme/bare']),
    ('package-abandoned', vendor_project(), ['show', 'acme/abandoned']),
    ('package-abandoned-json', vendor_project(), ['show', 'acme/gone', '-f', 'json']),
    ('package-meta', vendor_project(), ['show', 'acme/meta']),
    ('package-version', vendor_project(), ['show', 'acme/gamma', '1.2.0']),
    ('package-other-version', vendor_project(), ['show', 'acme/gamma', '2.0']),
    ('package-name-only', vendor_project(), ['show', 'acme/gamma', '-N']),
    ('package-path', vendor_project(), ['show', 'acme/gamma', '-P']),
    ('package-missing', vendor_project(), ['show', 'acme/nope']),
    # The root.
    ('self', vendor_project(), ['show', '--self']),
    ('self-json', vendor_project(), ['show', '-s', '-f', 'json']),
    ('self-name-only', vendor_project(), ['show', '--self', '--name-only']),
    # The platform.
    ('platform', vendor_project(), ['show', '--platform']),
    ('platform-json', vendor_project(), ['show', '-p', '-f', 'json']),
    ('platform-package', vendor_project(), ['show', '-p', 'ext-json']),
    ('platform-config', vendor_project(manifest={**ROOT_JSON, 'config': {'platform': {'php': '8.1.5', 'ext-fake': '1.0'}}}), ['show', '-p', 'php']),
    # The lock.
    ('locked', lock_project(), ['show', '--locked']),
    ('locked-direct', lock_project(), ['show', '--locked', '-D', '--no-dev']),
    ('locked-json', lock_project(), ['show', '--locked', '--format=json']),
    ('locked-package', lock_project(), ['show', '--locked', 'acme/alpha']),
    ('locked-tree', lock_project(), ['show', '--locked', '--tree']),
    ('locked-without-lock', vendor_project(), ['show', '--locked']),
    # Nothing installed.
    ('nothing-installed', {'composer.json': j(ROOT_JSON)}, ['show']),
    ('nothing-required', {'composer.json': j({'name': 'acme/app', 'repositories': NO_PACKAGIST})}, ['show']),
    # Options that don't go together, and a bad format.
    ('direct-and-platform', vendor_project(), ['show', '-D', '-p']),
    ('tree-and-path-package', vendor_project(), ['show', '-t', '-P', 'acme/alpha']),
    ('format-invalid', vendor_project(), ['show', '--format=xml']),
    ('ignore-without-outdated', vendor_project(), ['show', '--ignore=acme/alpha', '-N']),
    # No composer.json.
    ('no-composer-json', {}, ['show', '-N', '--platform']),
]


def composer(cwd, args, home, extra_env=None):
    env = {'PATH': os.environ['PATH'], 'HOME': home, 'COMPOSER_HOME': os.path.join(home, '.composer'),
           'COMPOSER_CACHE_DIR': os.path.join(home, '.cache'), 'COMPOSER_ROOT_VERSION': '1.0.0', 'COLUMNS': '500',
           **(extra_env or {})}
    env = {k: v for k, v in env.items() if v is not None}
    return subprocess.run(['php', PHAR, *args], cwd=cwd, env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL,
                          timeout=60)


def exception_message(stderr):
    """The lines of Composer's exception block (`In X line N:` and the message), or of Symfony's own error, without the
    command's usage."""
    lines = stderr.splitlines()
    start = next((i for i, l in enumerate(lines) if re.match(r'^In \S+ line \d+:$', l)), None)
    if start is None:
        if re.search(r'\n\nshow \[', stderr):
            return [l.strip() for l in stderr.split('\n\n')[0].splitlines() if l.strip()]
        return None
    message = []
    for line in lines[start + 1:]:
        if line.startswith('show [') or line.startswith('Exception trace:'):
            break
        text = line[2:].rstrip() if line.startswith('  ') else line.rstrip()
        if re.match(r'^In \S+ line \d+:$', text):
            continue
        if text and not re.fullmatch(r'\[[\w\\]+\]', text):
            message.append(text)
    return message


def record(name, files, args, extra_env=None):
    home = tempfile.mkdtemp(prefix='pun-show-home-')
    work = tempfile.mkdtemp(prefix='pun-show-')
    try:
        for path, content in files.items():
            file = os.path.join(work, path)
            os.makedirs(os.path.dirname(file), exist_ok=True)
            with open(file, 'w') as f:
                f.write(content)
        r = composer(work, args, home, extra_env)
        here = os.path.realpath(work)
        case = {'name': name, 'files': files, 'args': args, 'exit': r.returncode,
                'stdout': r.stdout.replace(here, '{dir}').replace(work, '{dir}'),
                'stderr': r.stderr.replace(here, '{dir}').replace(work, '{dir}')}
        if extra_env:
            case['env'] = extra_env
        message = exception_message(case['stderr'])
        if 'Composer could not find' in case['stderr']:
            # Composer prints this error itself, outside an exception block; it is still a run error.
            message = [line for line in case['stderr'].splitlines() if line]
        if message is not None and r.returncode != 0:
            # The test reads only these lines of a run error.
            case['message'] = message
            case['stderr'] = ''
        assert '/var/folders' not in json.dumps(case) and home not in json.dumps(case), (name, case)
        return case
    finally:
        shutil.rmtree(home)
        shutil.rmtree(work)


def probe_script():
    source = open(PLATFORM_RS).read()
    return re.search(r'const PROBE: &str = r#"(.*?)"#;', source, re.S).group(1)


def main():
    version = subprocess.run(['php', PHAR, '--version'], capture_output=True, text=True).stdout
    assert 'Composer version 2.11.2 ' in version, version
    probed = subprocess.run(['php', '-d', 'display_errors=stderr', '-r', probe_script()], capture_output=True, text=True, check=True)
    probe = json.loads(probed.stdout)
    probe['ini'] = []
    probe['inputs'] = []
    cases = [record(*c) for c in CASES]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, 'w') as f:
        json.dump({'probe': probe, 'cases': cases}, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'{OUT}: PHP {probe["php"]}, {len(cases)} cases')


if __name__ == '__main__':
    main()
