#!/usr/bin/env python3
"""Write crates/pun-cli/tests/fixtures/why.json, the manifest of crates/pun-cli/tests/why.rs: small projects with what
`composer depends` (`why`) and `composer prohibits` (`why-not`) 2.11.2 (.oracle/composer, the Composer the plugin host
pins) print and return for them, and the platform they ran on.

Both commands look at the platform too (`why php`, a requirement of a version the platform can't meet), so the
manifest records that platform once, as Pun's own probe (the PROBE script of
crates/pun-packages/src/resolver/platform.rs) run on the same `php`, without its `ini` and `inputs` (local paths). The
test gives Pun that probe through PHP_BINARY, a script that prints it.

`why-not` looks a version that isn't installed up in the project's repositories. So that no case reaches the network,
every root turns packagist.org off and lists the versions it may find in an inline `package` repository.

Each case is a set of files and the words after `composer`, run in a fresh temporary directory with a fresh HOME and
COMPOSER_HOME, COMPOSER_ROOT_VERSION=1.0.0 and COLUMNS=500, without a terminal on stdin. A case records `exit`,
`stdout` and `stderr`, and `message` (the lines of Composer's exception block) when Composer stops with an exception,
which Pun prints as its own run error (exit 2).

usage: scripts/export-why-cases.py   (needs php 8.5 and .oracle/composer from scripts/oracle.sh)
"""
import json, os, re, shutil, subprocess, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PHAR = os.path.join(ROOT, '.oracle/composer/composer.phar')
OUT = os.path.join(ROOT, 'crates/pun-cli/tests/fixtures/why.json')
PLATFORM_RS = os.path.join(ROOT, 'crates/pun-packages/src/resolver/platform.rs')


def j(value):
    return json.dumps(value, indent=4) + '\n'


def package(name, version, **fields):
    normalized = re.sub(r'^v', '', version)
    normalized = normalized if normalized.startswith('dev-') else normalized + '.0' * (3 - normalized.count('.'))
    p = {'name': name, 'version': version, 'version_normalized': normalized, 'type': 'library'}
    p.update({k.replace('_', '-'): v for k, v in fields.items()})
    return p


def installed(*packages, dev=()):
    """vendor/composer/installed.json and each package's directory (Composer drops a package whose directory isn't
    there)."""
    files = {'vendor/composer/installed.json': j({'packages': list(packages), 'dev': True, 'dev-package-names': list(dev)})}
    for p in packages:
        files[f'vendor/{p["name"].lower()}/.keep'] = ''
    return files


def lock(packages=(), packages_dev=(), **fields):
    return j({'content-hash': 'x', 'packages': list(packages), 'packages-dev': list(packages_dev), **fields})


ALPHA = package('acme/alpha', '1.0.0', require={'acme/gamma': '^1.0', 'psr/log': '^1.0 || ^2.0', 'php': '>=7.4', 'ext-json': '*',
                                               'psr/log-implementation': '1.0', 'acme/old-thing': '^1.0'},
                homepage='https://alpha.example')
BETA = package('acme/beta', '1.0.0', require={'acme/gamma': '~1.2', 'psr/log': '^1.0'}, conflict={'acme/delta': '<2.0'},
               source={'type': 'git', 'url': 'https://github.com/acme/beta.git', 'reference': 'x'})
GAMMA = package('acme/gamma', '1.2.3', require={'psr/log': '*'}, description='The gamma package')
DELTA = package('acme/delta', '1.5.0')
LOG = package('psr/log', '1.1.4', description='Common interface for logging libraries')
IMPL = package('acme/impl', '1.0.0', provide={'psr/log-implementation': '1.0.0'})
REPLACER = package('acme/replacer', '1.0.0', replace={'acme/old-thing': '1.0.0'})
C1 = package('acme/c1', '1.0.0', require={'acme/c2': '*'})
C2 = package('acme/c2', '1.0.0', require={'acme/c1': '*'})
DEV = package('acme/dev', '1.0.0', require={'psr/log': '^1.1'})
PACKAGES = [ALPHA, BETA, GAMMA, DELTA, LOG, IMPL, REPLACER, C1, C2, DEV]

# The versions why-not may find, in the root's inline repository.
AVAILABLE = [
    package('psr/log', '3.0.0', require={'php': '>=8.0.0'}),
    package('acme/gamma', '2.0.0', require={'php': '>=9.0', 'ext-nope': '*', 'psr/log': '^3.0'}),
    package('acme/delta', '1.0.0'),
    package('acme/alpha', '2.0.0', require={'acme/gamma': '^2.0'}),
]
NO_PACKAGIST = [{'packagist.org': False}] + [{'type': 'package', 'package': p} for p in AVAILABLE]
ROOT_JSON = {'name': 'acme/app', 'require': {'acme/alpha': '^1.0', 'acme/beta': '^1.0', 'php': '>=8.1', 'acme/c1': '*'},
             'require-dev': {'acme/dev': '*', 'psr/log': '^1.1'}, 'repositories': NO_PACKAGIST}


def vendor_project(manifest=None, packages=PACKAGES):
    return {'composer.json': j(ROOT_JSON if manifest is None else manifest), **installed(*packages, dev=['acme/dev'])}


def lock_project(manifest=None):
    return {'composer.json': j(ROOT_JSON if manifest is None else manifest),
            'composer.lock': lock([p for p in PACKAGES if p is not DEV], [DEV], **{'platform-overrides': {'php': '8.2.0'}})}


CASES = [
    # why (depends): a table, deepest dependents first; recursive; a tree.
    ('why', vendor_project(), ['why', 'psr/log']),
    ('depends', vendor_project(), ['depends', 'acme/gamma']),
    ('why-recursive', vendor_project(), ['why', 'acme/gamma', '-r']),
    ('why-tree', vendor_project(), ['why', 'psr/log', '--tree']),
    ('why-tree-ansi', vendor_project(), ['why', 'psr/log', '-t', '--ansi']),
    ('why-ansi', vendor_project(), ['why', 'acme/gamma', '--ansi']),
    ('why-case', vendor_project(), ['why', 'PSR/Log']),
    # Provided, replaced, the root itself, circular.
    ('why-provided', vendor_project(), ['why', 'psr/log-implementation']),
    ('why-replaced', vendor_project(), ['why', 'acme/old-thing', '-t']),
    ('why-root', vendor_project(), ['why', 'acme/app']),
    ('why-circular', vendor_project(), ['why', 'acme/c1', '--tree']),
    ('why-circular-table', vendor_project(), ['why', 'acme/c2', '-r']),
    # The platform.
    ('why-php', vendor_project(), ['why', 'php']),
    ('why-ext', vendor_project(), ['why', 'ext-json', '-t']),
    ('why-php-config-platform', vendor_project(manifest={**ROOT_JSON, 'config': {'platform': {'php': '8.1.5'}}}), ['why', 'php']),
    # Nothing depends on it, or it isn't there.
    ('why-nothing', vendor_project(), ['why', 'acme/delta']),
    ('why-not-found', vendor_project(), ['why', 'acme/nope']),
    ('why-nothing-installed', {'composer.json': j(ROOT_JSON)}, ['why', 'psr/log']),
    ('why-nothing-installed-no-requires', {'composer.json': j({'name': 'acme/app', 'repositories': NO_PACKAGIST})}, ['why', 'php']),
    # From the lock.
    ('why-locked', lock_project(), ['why', 'psr/log', '--locked']),
    ('why-locked-php', lock_project(), ['why', 'php', '--locked']),
    ('why-locked-without-lock', vendor_project(), ['why', 'psr/log', '--locked']),
    # why-not (prohibits): what keeps a version out.
    ('why-not', vendor_project(), ['why-not', 'psr/log', '3.0']),
    ('prohibits', vendor_project(), ['prohibits', 'psr/log', '^3.0', '-t']),
    ('why-not-recursive', vendor_project(), ['why-not', 'psr/log', '3.0', '--recursive']),
    ('why-not-ansi', vendor_project(), ['why-not', 'psr/log', '3.0', '--ansi']),
    ('why-not-installed', vendor_project(), ['why-not', 'psr/log', '1.1.4']),
    ('why-not-platform-requirement', vendor_project(), ['why-not', 'acme/gamma', '2.0']),
    ('why-not-root-requirement', vendor_project(), ['why-not', 'acme/alpha', '2.0']),
    ('why-not-conflict', vendor_project(), ['why-not', 'acme/delta', '1.0']),
    ('why-not-replaced', vendor_project(), ['why-not', 'acme/old-thing', '2.0']),
    ('why-not-unknown', vendor_project(), ['why-not', 'acme/nope', '1.0']),
    ('why-not-unknown-version', vendor_project(), ['why-not', 'acme/beta', '7.0']),
    ('why-not-php', vendor_project(), ['why-not', 'php', '7.0']),
    ('why-not-php-found', vendor_project(), ['why-not', 'php', '>=8.0']),
    ('why-not-ext', vendor_project(), ['why-not', 'ext-json', '1.0']),
    ('why-not-nothing', vendor_project(), ['why-not', 'acme/delta', '3.0']),
    ('why-not-locked', lock_project(), ['why-not', 'php', '9.0', '--locked']),
    ('why-not-missing-version', vendor_project(), ['why-not', 'psr/log']),
    # No composer.json.
    ('no-composer-json', {}, ['why', 'psr/log']),
]


def composer(cwd, args, home):
    env = {'PATH': os.environ['PATH'], 'HOME': home, 'COMPOSER_HOME': os.path.join(home, '.composer'),
           'COMPOSER_CACHE_DIR': os.path.join(home, '.cache'), 'COMPOSER_ROOT_VERSION': '1.0.0', 'COLUMNS': '500'}
    return subprocess.run(['php', PHAR, *args], cwd=cwd, env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL,
                          timeout=60)


def exception_message(stderr):
    """The lines of Composer's exception block (`In X line N:` and the message), or of Symfony's own error, without the
    command's usage."""
    lines = stderr.splitlines()
    start = next((i for i, l in enumerate(lines) if re.match(r'^In \S+ line \d+:$', l)), None)
    if start is None:
        if re.search(r'\n\n(?:depends|prohibits) \[', stderr):
            return [l.strip() for l in stderr.split('\n\n')[0].splitlines() if l.strip()]
        return None
    message = []
    for line in lines[start + 1:]:
        if re.match(r'^(?:depends|prohibits) \[', line) or line.startswith('Exception trace:'):
            break
        text = line[2:].rstrip() if line.startswith('  ') else line.rstrip()
        if re.match(r'^In \S+ line \d+:$', text):
            continue
        if text and not re.fullmatch(r'\[[\w\\]+\]', text):
            message.append(text)
    return message


def record(name, files, args):
    home = tempfile.mkdtemp(prefix='pun-why-home-')
    work = tempfile.mkdtemp(prefix='pun-why-')
    try:
        for path, content in files.items():
            file = os.path.join(work, path)
            os.makedirs(os.path.dirname(file), exist_ok=True)
            with open(file, 'w') as f:
                f.write(content)
        r = composer(work, args, home)
        here = os.path.realpath(work)
        case = {'name': name, 'files': files, 'args': args, 'exit': r.returncode,
                'stdout': r.stdout.replace(here, '{dir}').replace(work, '{dir}'),
                'stderr': r.stderr.replace(here, '{dir}').replace(work, '{dir}')}
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
