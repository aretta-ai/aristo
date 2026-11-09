#!/usr/bin/env python3
"""Every path a script opens, in one place (0.2.183, CONTRACT.md "Conventions", Rule B.1).

A script imports a name from here and never writes a path literal; scripts/validate.py refuses a path literal in any
other script (the data modules and scripts/mutants.py's mutant rows excepted). Each name is a path relative to the
repository root (the directory every script runs from), unless it says it is relative to the generated tree (GEN_*,
under GENERATED) or to a conductor checkout (CONDUCTOR_*).

    python3 scripts/layout.py NAME [ARG ...]    print one name (a tuple one item per line; a function called with ARGs),
                                                for the shell scripts
"""
import sys

# -- the repository's own files
VERSION = 'VERSION'
DECISIONS = 'DECISIONS.md'
CONTRACT = 'CONTRACT.md'
SCRIPTS = 'scripts'
VALIDATE = SCRIPTS + '/validate.py'
GENERATE_ROUTES = SCRIPTS + '/generate-routes.sh'
GENERATE_TYPES = SCRIPTS + '/generate-types.sh'
TYPES_DELTA_SCRIPT = SCRIPTS + '/types-delta.py'
SCRIPTS_GLOB = SCRIPTS + '/*.py'
VOCAB_GLOB = SCRIPTS + '/*_vocab.py'     # the data modules
GENERATE_CODES_SCRIPT = SCRIPTS + '/generate-codes.py'
GENERATE_LEDGER_KINDS_SCRIPT = SCRIPTS + '/generate-ledger-kinds.py'
GENERATE_POSTABLE_SCRIPT = SCRIPTS + '/generate-postable.py'
GENERATE_STORES_SCRIPT = SCRIPTS + '/generate-stores.py'
ALL_JSON = '**/*.json'     # every file of a kind, from the root (validate's parse pass)
ALL_YAML = '**/*.yaml'
ALL_TOML = '**/*.toml'

# -- the route registry and the planes' contracts
ROUTES = 'routes'
REGISTRY = ROUTES + '/registry.yaml'
PLANES = ('customer', 'admin', 'box')            # the planes with an openapi file


def openapi(plane):
    return f'{ROUTES}/{plane}/openapi.yaml'


def schemas_dir(plane):
    return f'{ROUTES}/{plane}/schemas'


def schema(plane, name):
    return f'{schemas_dir(plane)}/{name}.schema.json'


def generated_schemas_dir(plane):
    return f'{ROUTES}/{plane}/generated'


def generated_schema(plane, name):
    """an enum schema file (scripts/enum_schemas.py), the component's own name"""
    return f'{generated_schemas_dir(plane)}/{name}.json'


def openapi_ref_generated(name):
    """the `$ref` an openapi file's components/schemas line names an enum schema file by (relative to the openapi file)"""
    return f'./generated/{name}.json'


def openapi_ref_schema(name):
    """the `$ref` an openapi file's components/schemas line names a schemas/ file by"""
    return f'./schemas/{name}.schema.json'


CUSTOMER_OPENAPI, ADMIN_OPENAPI, BOX_OPENAPI = (openapi(p) for p in PLANES)
SCHEMA_PLANES = ('customer', 'admin', 'platform')   # the planes with a schemas/ dir
ROUTE_SCHEMAS_GLOB = ROUTES + '/*/schemas/*.schema.json'
GENERATED_SCHEMAS_GLOB = ROUTES + '/*/generated/*'
ENUM_SCHEMA_ID = 'https://aretta.ai/spec/' + ROUTES + '/{plane}/generated/{name}.json'

# -- the state files
STATE = 'state'
STATE_SCHEMAS_GLOB = STATE + '/**/*.schema.json'
CONFIG_REPO = STATE + '/config-repo'
REQUEST_SCHEMA = CONFIG_REPO + '/request.schema.json'
NOTIFY_SCHEMA = CONFIG_REPO + '/notify.schema.json'
CONFIG_REPO_EXAMPLES = (   # (example, its schema), each under CONFIG_REPO
    (CONFIG_REPO + '/repos/gobi.toml', CONFIG_REPO + '/repos/repo.schema.json'),
    (CONFIG_REPO + '/image.toml', CONFIG_REPO + '/image.schema.json'),
    (CONFIG_REPO + '/contacts.toml', CONFIG_REPO + '/contacts.schema.json'),
    (CONFIG_REPO + '/notify.toml', NOTIFY_SCHEMA))
REQUEST_SCHEMA_URLS = (   # every URL a relative $ref to the request schema can resolve to (no network)
    'https://aretta.ai/spec/request.schema.json', 'https://aretta.ai/spec/config-repo/request.schema.json',
    'request.schema.json', '../request.schema.json')
CONFIGEVENTS_SCHEMA = STATE + '/ledger/configevents.schema.json'
CONFIGEVENTS_EXAMPLE = STATE + '/ledger/configevents.example.jsonl'
EVENTS_SCHEMA = STATE + '/ledger/events.schema.json'
ORG_SCHEMA = STATE + '/live/org.schema.json'
ORG_DEFAULT_TOML = STATE + '/live/org.default.toml'   # 0.2.209: validate reads the default rate and ceiling from it
ORG_TOML = STATE + '/live/org.toml'
CONDUCTOR_SCHEMA = STATE + '/live/conductor.schema.json'
CONDUCTOR_TOML = STATE + '/live/conductor.toml'
BOX_CONFIG_SCHEMA = STATE + '/box/config.schema.json'
PENDING_INSTALLATIONS_DIR = 'state/installations/pending'   # 0.2.195 §1: the conductor's pending App installations, one <installation_id>.json each
PENDING_INSTALLATION_SCHEMA = STATE + '/platform/pending-installation.schema.json'   # 0.2.195 §1: one pending record (closed)
CREDENTIAL_SCHEMA = STATE + '/credentials/credential.schema.json'
CREDENTIALS_EXAMPLE = STATE + '/credentials/credentials.example.jsonl'
STORES_DIR = STATE + '/stores'
STORES_VOCABULARY = STORES_DIR + '/vocabulary.json'
STORES_README = STORES_DIR + '/README.md'
STORES_SCHEMA_GLOB = STORES_DIR + '/*/schema.json'


def store_dir(store):
    return f'{STORES_DIR}/{store}'


def store_schema(store):
    return f'{store_dir(store)}/schema.json'


def store_migrations(store):
    return f'{store_dir(store)}/migrations.json'


def tenant_store_file(store):
    """the file a store's schema.json names: the store's database in the tenant directory"""
    return f'tenants/<slug>/{store}.db'


# -- the consumers' types
TYPES = 'types'
TYPES_VERSION = TYPES + '/VERSION'
TYPES_DELTA = TYPES + '/delta.json'
TYPES_EXPECTED = TYPES + '/expected'


def types_dts(plane):
    return f'{TYPES}/{plane}.d.ts'


def types_expected(version):
    return f'{TYPES_EXPECTED}/{version}.diff'


# -- CI's door-type stub (B13)
STUB_MANIFEST = 'ci/door-types-stub/Cargo.toml'
STUB_FACTS = 'ci/door-types-stub/src/facts.rs'

# -- the generated tree: GEN_* are relative to GENERATED (the generators' --out); CONDUCTOR_* to a conductor checkout
GENERATED = 'generated'
CONDUCTOR_DOORS_RS = 'crates/access/src/generated/doors.rs'
CONDUCTOR_ROUTES_RS = 'crates/routes/src/generated/routes.rs'
CONDUCTOR_RUST = (CONDUCTOR_DOORS_RS, CONDUCTOR_ROUTES_RS)   # the --conductor form's two files
GEN_DOORS_RS = 'conductor/' + CONDUCTOR_DOORS_RS
GEN_ROUTES_RS = 'conductor/' + CONDUCTOR_ROUTES_RS
GEN_CAPABILITIES_RS = 'conductor/crates/access/src/generated/capabilities.rs'
GEN_PERMISSIONS_RS = 'conductor/crates/access/src/generated/permissions.rs'
GEN_ROLE_SETS_RS = 'conductor/crates/auth/src/generated/role_sets.rs'
GEN_ROUTES_JSON = 'dashboard/contract/routes.json'
GEN_ROUTE_ID_ALIASES_JSON = 'dashboard/contract/route_id_aliases.json'   # 0.2.220 §8.1: the one-cut id renames
GEN_CLI_PATHS_RS = 'cli/crates/aristo-core/src/conductor/paths.rs'
GEN_PLUGIN_ROUTES = 'plugin/routes.toml'
GEN_MANIFEST = 'manifest.json'
GEN_CONDUCTOR_CODES_RS = 'conductor/crates/routes/src/generated/codes.rs'
GEN_CLI_CODES_RS = 'cli/crates/aristo-core/src/generated/codes.rs'
GEN_BOX_DIAGNOSTICS_RS = 'box/crates/core/src/generated/diagnostics.rs'
GEN_CODES_TS = 'dashboard/src/generated/codes.ts'
GEN_OPS_CODES = 'ops/codes.txt'
GEN_KINDS_RS = 'conductor/crates/ledger/src/generated/kinds.rs'
GEN_LEDGER_KINDS_TS = 'dashboard/src/generated/ledger_kinds.ts'
GEN_BOOKS_LEDGER_KINDS = 'books/ledger_kinds.txt'
GEN_BOOKS_FINDING_UPSTREAM_STATES = 'books/finding_upstream_states.txt'   # 0.2.220 §5 (E120): the books kit validates the amend verb against it
GEN_BOX_LABELS_RS = 'box/crates/core/src/generated/labels.rs'
GEN_CONDUCTOR_LABELS_RS = 'conductor/crates/routes/src/generated/labels.rs'
GEN_PLUGIN_LABELS = 'plugin/assets/labels.toml'
GEN_FDE_LABELS_TABLE = 'fde/labels-table.md'
GEN_POSTABLE_RS = 'conductor/crates/notify/src/generated/postable.rs'
GEN_POSTABLE_TS = 'dashboard/src/generated/postable.ts'
GEN_OPS_POSTABLE = 'ops/postable.txt'
GEN_ORG_KEYS_RS = 'conductor/crates/orgstate/src/generated/org_keys.rs'
GEN_CONDUCTOR_KEYS_RS = 'conductor/bins/conductord/src/generated/conductor_keys.rs'
GEN_BOX_CONFIG_KEYS_RS = 'box/crates/core/src/generated/config_keys.rs'
GEN_OPS_CONFIG_KEYS = 'ops/config-keys.md'
GEN_OPS_DEPLOY = 'ops/deploy.txt'   # 0.2.215 §2: the deploy gate's rule (ops-side data: deploy.sh reads it)
GEN_VOCABULARY_RS = 'conductor/crates/stores/src/generated/vocabulary.rs'
GEN_STORES_VOCAB_TS = 'dashboard/src/generated/stores_vocab.ts'


def generated(rel):
    """a GEN_* name as a path from the repository root"""
    return f'{GENERATED}/{rel}'


MANIFEST = generated(GEN_MANIFEST)
ROUTES_JSON = generated(GEN_ROUTES_JSON)
CODES_TS = generated(GEN_CODES_TS)
STORES_VOCAB_TS = generated(GEN_STORES_VOCAB_TS)


# the Rust files scripts/generate-routes.sh formats with the pinned rustfmt, in its order
RUSTFMT = tuple(generated(p) for p in (
    GEN_DOORS_RS, GEN_ROUTES_RS, GEN_CAPABILITIES_RS, GEN_PERMISSIONS_RS, GEN_ROLE_SETS_RS, GEN_CLI_PATHS_RS,
    GEN_CONDUCTOR_CODES_RS, GEN_CLI_CODES_RS, GEN_BOX_DIAGNOSTICS_RS, GEN_KINDS_RS, GEN_BOX_LABELS_RS,
    GEN_CONDUCTOR_LABELS_RS, GEN_POSTABLE_RS, GEN_ORG_KEYS_RS, GEN_CONDUCTOR_KEYS_RS, GEN_BOX_CONFIG_KEYS_RS,
    GEN_VOCABULARY_RS))
# the emitted codes.rs files CI compiles standalone (0.2.122)
CODES_RS = (generated(GEN_CONDUCTOR_CODES_RS), generated(GEN_CLI_CODES_RS))


if __name__ == '__main__':
    _v = globals().get(sys.argv[1]) if sys.argv[1:] else None
    if _v is None or sys.argv[1].startswith('_'):
        sys.exit(f'layout.py: no name {sys.argv[1:]!r}')
    _v = _v(*sys.argv[2:]) if callable(_v) else _v
    print('\n'.join(_v) if isinstance(_v, tuple) else _v)
