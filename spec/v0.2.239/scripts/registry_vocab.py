"""The closed vocabularies of routes/registry.yaml (T0 "names once"). scripts/validate.py checks
every row against them and scripts/generate-routes.py refuses a word outside them. A new word is
an edit here first, reviewed like any vocabulary change."""

PLANES = ('customer', 'admin', 'box', 'platform')
# routes.md §2, F1–F12 by name.
FAMILIES = ('identity', 'setup', 'live', 'mirrors', 'verify', 'picture', 'findings', 'books',
            'campaigns', 'intake', 'tenants', 'shell', 'notify',   # `notify` the 13th (T1, 0.2.110)
            'strands',   # 0.2.168: F14, the strand routes
            'pricing',   # 0.2.209: F15, the operator's pricing rows (the tenant rate and the job time limits)
            'clock',     # 0.2.212: F16, the operator's clock rows (the per-repo pause)
            'conversations')   # 0.2.220 §11.9 (E122.9): the chat and conversation rows leave `campaigns`
AT = ('repo', 'org_level', 'org_directory', 'org_root', 'admin', 'sign_in')
# 0.2.150 (0.6 slice 2): THE PERMISSION VOCABULARY replaces the rungs (RUNGS) and the capabilities as the door
# vocabulary. Each door row names ONE `permission` (or `authenticated`: any principal, no permission) and a `scope`.
# 0.2.156 (the auth spec, clean slate; the operator's final word, DECISIONS 2026-10-22T05:07:29Z): ELEVEN words. A ROUTE
# is one permission at the ONE scope its path names: a TENANT route at `repo` or `org`, a PLATFORM route at `platform`
# (an org or repo in its path is a parameter); no containment between the kinds. `launch` is `launch_campaign`,
# `campaign_author` is `curate_campaign`, `administer` is `manage_access`, and `staff` goes (the forge gate is `read` at
# `platform`). `formalize` FILES a finding (an internal record, never outward); `release` PUBLISHES it to the customer's
# GitHub (the issue, reject-repair's reopen and comment; 0.2.154).
PERMISSIONS = ('read', 'verify', 'launch_campaign', 'curate_campaign', 'release', 'configure', 'steer',   # 0.2.220 §11.6: formalize retires (E122.55)
               'manage_access', 'operate')   # 0.2.238 §1: stamp retires (nobody holds it; its two doors retire, §2); the registry's Permission enum minus STAMP
# 0.2.156: the retired permission words, kept on the WIRE only (Permission.json on both planes, DEPRECATED) for 0.x: no
# door row, holder or REQUIRES_LEGACY key names one.
PERMISSIONS_DEPRECATED = ('launch', 'campaign_author', 'administer', 'staff', 'formalize', 'stamp')   # 0.2.220 §11.6; 0.2.238 §1: stamp
AUTHENTICATED = 'authenticated'          # a door row's permission column when the row needs no permission (0.2.145)
SCOPES = ('repo', 'org', 'platform')
# Where each word may be required (0.2.156 §2): `read` at platform is entering Aretta's own surfaces (the forge gate);
# `steer` is Aretta's act on a tenant (the admin plane's per-org rows); `operate` is an act on Aretta itself, platform only.
PERMISSION_SCOPES = {
    'read': ('repo', 'org', 'platform'), 'verify': ('repo',), 'launch_campaign': ('repo',), 'curate_campaign': ('repo',),
    'release': ('repo',), 'configure': ('repo', 'org'), 'steer': ('repo', 'org'),
    'manage_access': ('repo', 'org'), 'operate': ('platform',),   # 0.2.238 §1: stamp retires
    AUTHENTICATED: ('org', 'platform'),
}
# 0.2.170 (edition ceremonies without a personal login; the door's Option A, DECISIONS 2026-10-24T17:39:35Z): a FIELD-LEVEL
# door. A route may require a further permission for a named value of a body field; the one decision function checks a
# FIELD_DOORS row after the row's own door, so the refusal is the ordinary `forbidden` with `requires`. Choosing the internal
# plane or the dev line on a verify start needs `steer` at the repo beside the row's `verify` (staff, operators and the seat
# hold it; a CI token and a customer member do not and keep external, main-line runs). This replaces the conductor's rule
# that a seat's run is always external and main-line.
# (re-marked f7f9e39a, the lane's G2): the line is the `X-Aristo-Line` request HEADER, never a body field; the plane is the
# body's `plane`. On verify.sessions.start only, naming any line, or `plane: internal`, needs `steer`; elsewhere the line
# selector keeps today's rule (a caller deemed external may name no line; Actor::is_external is not redefined). The
# external-on-dev diagonal stays 400 `line_not_selectable`.
FIELD_DOORS = (    # (row id, where, name, value, the extra permission that value needs at the row's scope)
    ('verify.sessions.start', 'body',   'plane',         'internal', 'steer'),
    ('verify.sessions.start', 'header', 'x-aristo-line', 'main',     'steer'),
    ('verify.sessions.start', 'header', 'x-aristo-line', 'dev',      'steer'),
    # 0.2.214 §1, §4 (arc-verify slice 2): naming an arc needs `steer`, the same door as `plane: internal`. The value column is the
    # typed PARSE of the field (a FORK_REFS word, BODY_GRAMMARS / QUERY_SELECTORS), not an enum word: `arc` = the value parses as
    # Arc(slug) per 0.2.213. Admission: `arc_ref` requires `plane: internal` and line `dev` (400 arc_ref_internal_only); a value
    # that is no Arc(slug) is 422 arc_ref_invalid; the runs-list `ref` filter from an external caller is 400 arc_filter_internal_only.
    ('verify.sessions.start', 'body',   'arc_ref',       'arc',      'steer'),
    ('verify.runs.list',      'query',  'ref',           'arc',      'steer'),
)
HEADER_SELECTORS = (('x-aristo-line', 'RUN_LINES'),)   # the request headers a FIELD_DOORS `header` entry may name, and the vocabulary tuple of their values
QUERY_SELECTORS = (('ref', 'FORK_REFS'),)   # 0.2.214 §4: the query parameters a FIELD_DOORS `query` entry may name, and the vocabulary tuple the value's typed parse comes from (`ref=dev-<slug>` parses as `arc`)
BODY_GRAMMARS = (('arc_ref', 'FORK_REFS'),)   # 0.2.214 §1: a FIELD_DOORS `body` field with no enum (a string): the vocabulary tuple its typed parse comes from (`arc_ref` parses as `arc`)
# WHO HOLDS WHAT (0.2.156 §3). A ROLE is a bundle of permissions with NO scope; a HOLDING is a role at a scope, made only
# by PROVENANCE (below). Exactly FOUR person roles: `member` and `admin` (customer), `staff` and `operator` (Aretta).
# Machines are not roles: a machine's fixed set is held at its reach. (holder, principal kind, permissions).
# 0.2.238 §1 (operator 2026-11-09: "the registry's view on the auth model is the correct one"): the SEVEN holders of the
# registry (model/auth.py HLD__MEMBER, HLD__ADMIN, HLD__STAFF, HLD__OPERATOR, HLD__SEAT, HLD__CI, HLD__BOX). `service` and
# `org_reader` leave: the in-process writers (service:<name>) are the conductor itself, a SUBMITTERS subject word on the ledger
# line, never a holder passing a door (the stamp-time admission reads "the write originates in process", not a holder); the
# Slack answerer is the conductor acting on an org whose replies consent is on (the consent check is the answer's precondition).
HOLDERS = {
    'member':             ('person', ('read', 'verify', 'launch_campaign', 'curate_campaign', 'release')),
    'admin':              ('person', ('read', 'verify', 'launch_campaign', 'curate_campaign', 'release', 'configure',
                                      'manage_access')),
    # every tenant word: staff hold it at the platform AND at every tenant org, standing (PROVENANCE)
    'staff':              ('person', ('read', 'verify', 'launch_campaign', 'curate_campaign', 'release',
                                      'steer', 'configure', 'manage_access')),
    'operator':           ('person', ('read', 'verify', 'launch_campaign', 'curate_campaign', 'release',
                                      'steer', 'configure', 'manage_access', 'operate')),
    'ci':                 ('machine', ('read', 'verify')),
    # 0.2.238 §1 (registry HLD__SEAT): the seat loses configure — it is Aretta: it STEERS what the customer configured (an image
    # through live.image.accept, which it holds); the `setup.image.put` rows keep their configure door (the customer's request)
    # and refuse a seat by the permission. No launch (ruling C); formalize retired (0.2.220 §11.6; not added this cut).
    'seat':               ('machine', ('read', 'verify', 'curate_campaign', 'steer')),
    # 0.2.200 §1 (E94 block N): the box is a MACHINE role at platform reach (its two calls, live.org.get and
    # live.image.get, are `read`). `ops` and `operator_token` leave: the ops role is eliminated (a staff or operator person acts
    # with their own `cli` credential); 0.2.238 §3: the operator token is the operator's own PERSON credential.
    'box':                ('machine', ('read',)),
}
# 0.2.156 §5: PROVENANCE, as data (it replaces the prose resolution and the staff rule). Only provenance makes holdings.
# 0.2.157 (the door's clarifications Q2–Q4, DECISIONS 2026-10-22T06:07:27Z and 06:08:42Z; vyasa's draft): two columns.
# (source, the roles it gives, where, on_repos, override).
#   where     `every_org` = at org scope on EVERY tenant org, Aretta-owned included (a standing holding; the kind flag
#             carries no access rule); `reach` = a minted credential's reach; `machine_set` = the machine's HOLDERS set.
#   on_repos  for a source that holds at an org: `all` = the holding applies on every repo of the org; `forge` = it stays
#             at org scope and reaches a repo only where the forge itself grants on it (as on GitHub). None for a source
#             that holds at a repo, the platform or a reach only.
#   override  `covered_scopes` = an `[[access]]` entry OVERRIDES the forge-derived holdings at every scope it covers
#             (an org-wide entry at the org and on every repo; a `repos` entry on those repos), up or down, unless the
#             entry says `floor = true` (then "at least"). None = the source is what is overridden, or overrides nothing.
#   forge_repo_member     a GitHub repo permission read, triage, write or maintain: member on that repo
#   forge_repo_admin      the GitHub repo permission admin: admin on that repo
#   forge_org_member      GitHub org membership: member at the org (on_repos forge)
#   forge_org_owner       GitHub org ownership: admin at the org (on_repos forge)
#   forge_outside_collaborator  repo access without org membership: the repo rows above on that repo, nothing at the
#                         org (FORGE_GRANT_SCOPES, 0.2.155)
#   access_entry          org.toml `[[access]]` {subject, role, repos?, floor?}: member or admin on the listed repos, or
#                         the org (on_repos all; override covered_scopes)
#   staff_config          conductor.toml `[[staff]]` (role `staff`, or `scientist` read as `staff`), or CampanileSkyForge
#                         membership: staff at the platform AND every tenant org (on_repos all)
#   staff_config_operator `[[staff]] role = "operator"`: operator at the platform AND every tenant org (on_repos all)
#   credential            a minted credential: the machine's set at its reach
# COMBINATION: a person's permissions at a scope are the union of the roles held there, after the overrides; an org
# holding applies on a repo of that org where its source says `all`. No containment: a platform holding never reaches a
# tenant route and a tenant holding never reaches a platform route. Generated into role_sets.rs.
PROVENANCE = (
    ('forge_repo_member', ('member',), ('repo',), None, None),
    ('forge_repo_admin', ('admin',), ('repo',), None, None),
    ('forge_org_member', ('member',), ('org',), 'forge', None),
    ('forge_org_owner', ('admin',), ('org',), 'forge', None),
    ('forge_outside_collaborator', ('member', 'admin'), ('repo',), None, None),
    ('access_entry', ('member', 'admin'), ('repo', 'org'), 'all', 'covered_scopes'),
    ('staff_config', ('staff',), ('platform', 'every_org'), 'all', None),
    ('staff_config_operator', ('operator',), ('platform', 'every_org'), 'all', None),
    ('credential', ('machine_set',), ('reach',), None, None),
)
PROVENANCE_AT = SCOPES + ('every_org', 'reach')
PROVENANCE_ON_REPOS = ('all', 'forge')
PROVENANCE_OVERRIDE = ('covered_scopes',)
# 0.2.155 (slice 4, the door's ruling 4): the permissions the kept any-repo-admin exception (0.2.150 §3: an `admin` on
# any repo of the org holds it at org scope) lifts. `configure` only; NEVER `manage_access` (0.2.156; was `administer`),
# which at org scope is held by ORG-LEVEL admin alone (the forge org's owner, or an `[[access]]` admin entry at the
# org). Generated into role_sets.rs.
ANY_REPO_ADMIN_LIFTS = ('configure',)
# 0.2.157 (the door, DECISIONS 2026-10-22T06:25:21Z (b)): the FORGE_GRANT_SCOPES relations the lift applies to. A GitHub
# ORG MEMBER only: an outside collaborator who is a repo admin gets nothing at the org (FORGE_GRANT_SCOPES governs, and the
# lift never crosses it). Generated into role_sets.rs beside ANY_REPO_ADMIN_LIFTS.
ANY_REPO_ADMIN_LIFTS_FOR = ('org_member',)
# 0.2.155 §1c(2) (the operator, 2026-10-22T02:07:53Z): the scopes a FORGE grant resolves at, by the grantee's relation to
# the forge org. An outside collaborator (forge access on a repo, not a member of the forge org) resolves on THAT repo
# only: admin if a repo admin, member for read, write or maintain, NOTHING at org scope (the 2026-10-07 word stands; §3b
# is withdrawn). Generated into role_sets.rs.
FORGE_GRANT_SCOPES = {'org_member': ('org', 'repo'), 'outside_collaborator': ('repo',)}
# 0.2.155 (slice 4): THE CREDENTIAL KINDS, one closed vocabulary. Each resolves to ONE principal: a `person` kind acts
# AS the person (its permissions are the person's, re-resolved per request; no HOLDERS entry), a `machine` kind (ci,
# seat, service) and a `platform` kind (box, ops, operator; ruling A) hold their HOLDERS sets. `conversation` is
# SPECIFIED only (its mint is 0.7's: no mint row). `operator` is RETIRING (two releases; its HOLDERS entry leaves in
# release 2).
# 0.2.200 §1 (E94 block N): `ops` retires; `box` is a machine kind. 0.2.238 §3 (registry CredentialKind.OPERATOR, Principal.PERSON):
# `operator` is the operator's own PERSON credential, presented through tooling as the cli token is; the transitional word
# `platform` and TRANSITIONAL_PRINCIPALS are gone.
# 0.2.220 §5 (E115; prod 0 records of kind service, ops 2026-11-01): `service` leaves: no credential presents it (every
# run.stamped is stamped in process). The HOLDERS word `service` is the conductor itself (its credential is the deployment's
# secrets; ledger submitter service:<component>).
CREDENTIAL_KINDS = ('session', 'cli', 'ci', 'seat', 'box', 'operator', 'conversation')
AUTHENTICATED_CREDENTIALS = ('session', 'cli', 'conversation')   # 0.2.238 §3 (FINAL v2): the credential kinds an `authenticated` door row accepts (registry PERSON_CREDENTIALS); any other kind, the operator token included, is refused 401 unauthenticated there
CREDENTIAL_KIND_PRINCIPAL = {
    'session': 'person', 'cli': 'person', 'conversation': 'person',
    'ci': 'machine', 'seat': 'machine', 'box': 'machine',
    'operator': 'person',   # 0.2.238 §3: a person credential (was the transitional word `platform`, 0.2.200)
}
CREDENTIAL_KIND_HOLDER = {'ci': 'ci', 'seat': 'seat', 'box': 'box'}   # a person kind (the operator token included, 0.2.238 §3) has none: it is the person
# Each kind's expiry: a conductor.toml `[tokens]` key (`tokens.<key>`), or a fixed rule. `operator` has none (retiring).
CREDENTIAL_EXPIRY = {
    'session': 'fixed: 12 h',
    'cli': 'tokens.cli_token_ttl_days; tokens.staff_cli_token_ttl_days for a person listed in [[staff]]',   # 0.2.200 §1
    'ci': 'tokens.ci_token_ttl_days',
    'seat': 'tokens.ci_token_ttl_days',
    'box': 'tokens.platform_token_ttl_days',
    'conversation': 'fixed: at most 1 h, renewed while the conversation is active, at most 12 h in total',
    'operator': None,
}
# 0.2.157 (the door, DECISIONS 2026-10-22T06:14:43Z Q2; cCred MEASURED on a copy of the store): a LEGACY person CLI
# token is a floor record of kind `ci` with NO `principal_subject` (0.2.160, the door's 16:38:39Z ruling (B): a `ci`
# record that carries `principal_subject` is a CI token, read by it, never legacy) whose subject is NOT its label (the CI mint sites write subject = label; `aristo
# login` writes the person's login). The label is read ONLY to classify a floor record, never as a subject. Such a record
# keeps the floor's machine semantics until `[tokens] legacy_cli_cutoff`, then is refused like an expired token.
# Generated into role_sets.rs.
LEGACY_CLI_PREDICATE = ('kind:ci', 'no:principal_subject', 'subject!=label')
# 0.2.161 (0.6 slice 5, §2; the door, DECISIONS 2026-10-22T09:16:08Z): REVOCATION BY PRINCIPAL. The `principal.revoked`
# ledger kind's triggers: an `[[access]]` entry REMOVED (live.platform.put), every forge grant lost and no `[[access]]`
# entry (the membership webhook or the periodic re-check), a `[[staff]]` entry gone (the boot reconcile; every org). A
# lowered role is a recompute, never a revocation; a failed lookup never revokes. 0.2.164: `staff_removed` per erratum E1
# (at the restart: operators by the list, scientists by a one-time membership check); the fold's chain, place and row
# rule per errata E3–E5 (REVOCATION_FOLD_CHAIN, REVOCATION_FOLD_AT, REVOCATION_ROW_WHEN below).
REVOCATION_TRIGGERS = ('access_removed', 'forge_lost', 'staff_removed')
# 0.2.161 §2.4: the periodic forge re-check's interval in hours, a spec CONSTANT (not a config key). RULED by the
# operator, DECISIONS 2026-10-22T15:30:01Z ("4a,b,c yes").
FORGE_RECHECK_HOURS = 1
# 0.2.164 (erratum E2): the maximum rows one `GET /api/client/v1/org/audit` answer carries, a spec CONSTANT (not a key;
# `?limit=` is not a parameter: one size, no knob); `WireAudit.next_since?` names the seq for the following page.
AUDIT_PAGE_ROWS = 200
# 0.2.164 (errata E3–E5; the door, DECISIONS 2026-10-23T10:50:16Z, 11:52:23Z, 13:15:30Z, 14:01:48Z, 14:23:03Z): the fold's
# rules as data (the operator's 2026-10-18 word: no text-scrape checks).
#   E3: a person credential whose record names NO org (a staff session, scopes []) folds on EVERY org chain for every
#       request, whatever org it names or none; a record that names an org folds on that org's chain only. The
#       `staff_removed` writers append the row on every tenant that has a chain.
#   E4: the fold runs where credential expiry runs, in credential resolution, BEFORE any door or org-scope check on every
#       plane: a revoked credential answers 401 revoked on every row, held or not; a door's 403 never precedes it.
#   E5: a `principal.revoked` row is appended on a chain only when the trigger's pass deleted a record of the subject
#       there, or a live record remains there that the fold does not already refuse; otherwise nothing.
REVOCATION_FOLD_CHAIN = (('no_org', 'every_org_chain'), ('org', 'that_org_chain'))
REVOCATION_FOLD_AT = 'credential_resolution_before_any_door'
REVOCATION_ROW_WHEN = ('record_deleted', 'live_unrefused_record_remains')
# 0.2.161 §3: SURFACE, a closed list, carried in the PAYLOAD beside `actor` on the route-written kinds
# (ledger_kinds_vocab SURFACE_KINDS); derived from the person credential kind that acted, `slack` through the Slack
# paths, absent for a machine principal.
SURFACES = ('dashboard', 'slack', 'cli', 'agent')
SURFACE_OF_KIND = (('session', 'dashboard'), ('cli', 'cli'), ('conversation', 'agent'), ('operator', 'cli'))   # the person credential kinds; 0.2.238 §3: the operator token is presented through tooling as the cli token is
# 0.2.163 (0.7 cut B, E5, the ALIAS table): ONE person principal (`github:<login>`, the sign-in provider's: the PRIMARY
# alias, the one hard-coded place; a provider-agnostic sign-in later changes only which alias is primary) and N provider
# subjects. An ALIAS subject `<provider>:<provider-specific id>` (Slack: `slack:<team_id>:<user_id>`) names a person's
# provider identity and is NEVER a principal: a request resolves an alias to its person and proceeds as the person, or
# does not proceed. Per org, in the tenant's identity store `tenants/<slug>/identity.db` (0.2.165 E6: STORES row
# `identity`; state/stores/identity/), its tables: `persons
# {person_subject PK, display_name?, created_at}` (created on first link or first sight, display_name NULL; set by the
# person through `identity.own_profile.put` since 0.2.166) and `aliases {alias_subject PK, person_subject FK, created_at, created_by, revoked_at?}` (at most one
# live alias per Slack user and one per person and provider, as constraints), written DIRECTLY by the link and unlink
# routes; every lookup reads the store, never a fold of the chain (the operator, 2026-10-23T07:25:37Z: the ledger is not a
# source of truth for auth). `<provider>.linked` / `<provider>.unlinked` (ledger_kinds_vocab) are its AUDIT and never
# carry a name. Closed; `okta`, `google` are later rows of the same table.
ALIAS_PROVIDERS = ('slack',)
ALIAS_SUBJECT_GRAMMAR = '<provider>:<provider-specific id>'
# 0.2.163 §B.3a: the records the ONE revocation predicate (0.2.161) governs, each with where its `created_at` is read: a
# credential record's own `created_at` (the credential store); an alias row's `created_at`, its link time (identity.db).
# Revocations move INTO identity.db in the cut-B conductor lane (the operator's (a), 07:31:45Z); this cut reads them from
# slice 5's fold.
# A record is valid only if no `principal.revoked` of its person on that org (slice 5's fold) is at or after it; a revoked
# alias regains power only by a re-link.
# 0.2.165 E6: the alias entry names identity.db (there is no org.db).
REVOCATION_RECORDS = (('credential', 'credential.created_at'), ('alias', 'identity.db aliases.created_at'))
# 0.2.166 (USER SETUP, first element; vyasa's FINAL SPEC-0.2.166-own-profile-display-name.md; the operator 2026-10-24T03:16:14Z):
# the person sets their OWN display name, identity.db `persons.display_name` (`identity.own_profile.put`). The rule as data
# (the door, 2026-10-24T03:19:25Z): ONE rule for the conductor's check, the openapi's maxLength/pattern and the dashboard's
# sentence. The conductor applies its rules in DISPLAY_NAME_ORDER (0.2.167 erratum B: not_subject before classes).
DISPLAY_NAME = (                      # registry_vocab; ONE rule for the conductor's check, the openapi's maxLength/pattern and the dashboard's sentence
    ('min', 1),                       # after trimming; empty → clear is `null`, not ""
    ('max', 64),                      # Unicode scalar values, not bytes
    ('classes', ('L', 'M', 'Nd', 'Zs', ".-'_")),   # allowed: Unicode letters (any script), combining marks, decimal digits, the space separator, and . - ' _
    ('forbidden', ('Cc', 'Cf', 'Zl', 'Zp', '<>')), # control and format characters, line/paragraph separators, markup brackets (every other class is outside `classes` and refused by that)
    ('trim', True),                   # leading/trailing Zs removed before every other rule; internal runs of spaces kept as typed
    ('not_subject', True),            # never the subject form: a value matching <provider>:<handle> for github or any ALIAS_PROVIDERS member is refused (the rendering rule: never "github:alice")
    ('code', 'invalid'),              # 400, customer layer, `field: display_name`, `rule` ∈ DISPLAY_NAME_RULES — an existing code, nothing new
)
DISPLAY_NAME_RULES = ('min', 'max', 'classes', 'not_subject')   # the `rule` word the 400 carries; the dashboard words one sentence per rule (U12)
# 0.2.167 erratum B (cB measured at v0.2.166, 2026-10-24T07:32:28Z): 0.2.166's order ran classes first, so `github:alice`
# refused with `rule: classes` (`:` is outside the classes) and not_subject never fired. The order is data.
DISPLAY_NAME_ORDER = ('trim', 'min', 'max', 'not_subject', 'classes')   # the conductor applies DISPLAY_NAME's rules in exactly this order; the first that fails names the 400's `rule`
# the `change` word of a `person.renamed` payload (ledger_kinds_vocab): a string SETS, `null` CLEARS
RENAME_CHANGES = ('set', 'cleared')


def display_name_pattern():
    """The pattern GENERATED from DISPLAY_NAME (0.2.166 §1): each Unicode general category of `classes` as \\p{..}, its
    literal characters escaped, bounded by `min` and `max`. The openapi's WireOwnProfilePut and the generated Rust and
    TypeScript carry this string; validate builds it here and holds the schema to it."""
    rule = dict(DISPLAY_NAME)
    parts = []
    for c in rule['classes']:
        if c[:1].isupper() and c.isalpha():
            parts.append('\\p{' + c + '}')
        else:
            parts.append(''.join('\\' + ch if ch in '\\]^-' else ch for ch in c))
    return '^[' + ''.join(parts) + ']{' + f"{rule['min']},{rule['max']}" + '}$'


# 0.2.165 (the per-tenant stores; vyasa's FINAL SPEC-0.2.165-org-db.md; the operator 2026-10-24 "think properly and come up
# with a clean architecture"): ONE SQLite file per BOUNDED CONTEXT per tenant, `tenants/<slug>/<store>.db`, named by what it
# holds, its own migration chain (state/stores/<store>/migrations.json; the declared result in schema.json), written
# DIRECTLY by its routes, snapshotted into the DR mirror and the backup. One class: no store is rebuilt from the chain; a
# transaction never spans two stores; cross-store references are ids and subjects, never foreign keys. The chain holds the
# FACTS and audits every store write (store first, audit beside). No shared `org.db` (E6). (store, owner)
STORES = (
    ('identity',  'the link/unlink routes'),
    ('strands',   'the strand routes; the producers'),
    ('runs',      'the verify dispatcher'),
    ('editions',  'the promote route; the convergence'),
    ('findings',  'curation routes, matcher, report'),
    ('bugs',      'curation routes'),
    ('campaigns', 'the campaign saga'),
    ('pulls',     'the publisher'),
    # 0.2.167 (vyasa's FINAL SPEC-0.2.167-credits-store.md §1; the operator 2026-10-24, "yes do it"): credits are their own
    # bounded context, a pool drawn by many features (a launch and a chat message today; 0.7b's agent turns next)
    ('credits',   'the credit routes; every spender through the draw rule'),
)
# 0.2.167 §0: the stores a conductor at this spec version opens, creates and migrates. The runner opens only these (a
# STORES row outside it has no file yet); the backup's staged-copy list and the mirror allow-list read it too (ops' hand
# step is the same list). validate: a subset of STORES, in STORES order.
STORES_LIVE = ('identity', 'strands', 'runs', 'editions', 'findings', 'bugs', 'campaigns', 'credits')   # 0.2.224 §1 (E124 (50)): `editions` joins (m1–m4: the edition rows, tracks m3, images m4; the cut C writes land in the stored rows from this tag); 0.2.179 §1: `runs` and `bugs` join (fold retirement release 1); 0.2.175 E19: `findings` joins (deferred_writes); 0.2.168 §2: `strands` joins (a conductor at v0.2.168 creates strands.db)
# 0.2.179 §1 (the team-lead's three release-1 rulings, consistent with tenant-footprint.md §2.5): every write route writes its
# store rows in one transaction, then appends the audit record, then stores that record's seq in the row's `ledger_seq` where
# the table has one. Reads stay on the fold in release 1, through ONE door, so a floor binary (whose writes reach only the
# chain) is seen at once and the store is proven before it is read; release 2 flips that door to the store once the
# store-vs-fold diff has stayed at zero. Two stated exceptions inside release 1: `next_run_seq` takes max(store, fold); a route
# whose audit append fails answers 5xx and keeps its store row with `ledger_seq` NULL for the reconciler or the client's
# idempotent retry. Release ordering (0.2.178 §2): this cut's findings backfill lands before failures_explained is enforced.; the stores a conductor at this spec version opens, creates and migrates; ⊆ STORES; each later cut adds its own store when its first writer ships (strands at 0.2.168)
# the folds that retire (tenants/<slug>/derived/<name>.json + the in-memory projection store): deleted in release two
RETIRED_FOLDS = ('runs', 'bugs', 'disclosure-gate', 'findings', 'capabilities', 'first-served',
                 'reports')   # 0.2.179 §6: derived/reports.json (stale: no writer, lagging head) is deleted in release 2 too
# 0.2.205 (fold retirement release 2): "the fold code retires" means exactly the in-memory projection store for the RETIRED_FOLDS
# and their derived/*.json files; the fold FUNCTIONS stay (the row computation of the store-first write, the boot catch-up, the
# diff). A boot step deletes derived/<fold>.json for exactly the RETIRED_FOLDS words per tenant (the files, not an unmarked
# store's in-memory fallback), idempotent, one journal INFO line per tenant that had any file. The read door answers from the
# store for every projection a store table answers and from the residual fold for a RESIDUAL_FOLDS projection. A store is read
# only when every one of its backfill marks exists; an UNMARKED store (a deferred backfill, E42) keeps the fold as its fallback,
# one WARN per org per boot naming the store and the missing mark; a marked store drops its in-memory cache. Two read rules the
# stores reproduce: an amended run sorts by its stamp seq, not the amendment's ledger_seq; a finding's disclosure_status counts
# only disclosures AFTER its first curation (turso F-023). Writes stay store-first plus audit as in release 1 (a rollback is safe
# by construction: the floor reads the fold it still builds from the chain).
# 0.2.210 (fold retirement release 2, product-v2 row 5): the orders the stores must reproduce when the read door flips, typed
# from cSwap's code-read census (sprint-0.7/r2-census/read-order.tsv 7d75e5a3; conductor main fb22901f). Paging: `next` is the
# key of the page's last row only when a row exists past the page (peek limit + 1), else null; an amended run sorts by its stamp
# seq, not the amendment's ledger_seq (0.2.205). The conductor's test oracle reads the tuple (vocabulary.rs).
READ_ORDER_RULES = (   # (rule, reader, key, direction, cursor param, cursor type, cursor test, limit default, limit max)
    ('runs.order',     'verify.runs.list and every runs reader', 'the stamp chain seq (run.stamped / run.imported envelope seq) = run_seq', 'descending', None,    None,      None,          None, None),
    ('runs.page',      'verify.runs.list',                     'runs.order',                                                             'descending', 'after', 'integer', 'run_seq < after', 50,  500),
    ('runs.get',       'the run read',                         '(repo, run_seq)',                                                        'first match in chain order: on a duplicate run_seq (a remint) the EARLIEST stamp answers', None, None, None, None, None),
    ('findings.page',  'the story and internal findings pages', 'finding id (string)',                                                   'ascending (byte order)', 'after', 'string', 'id > after', 50, None),
    ('history.ordinal','a finding\'s history',                 'the chain seq of each act',                                              'ascending; one act per record per finding (a disclosure naming several findings gives each one act)', None, None, None, None, None),
    ('for_repo.latch', 'the repo\'s findings',                 'finding id',                                                             'ascending; the repo\'s findings are those whose bucket is the repo; the latch keeps every first_served entry whose finding is in the repo\'s bucket or edition-only', None, None, None, None, None),
    ('next_run_seq',   'the verify dispatch',                  'max run_seq for (org, repo)',                                            'release 1: max(fold, store MAX(run_seq) WHERE repo) + 1, 1 for a first run; release 2: store MAX + 1 under the (org, repo) lock', None, None, None, None, None),
)
# a reader is a registry row (the reader's first word is its id) or one of these named in-process readers
READ_ORDER_IN_PROCESS_READERS = ('the run read', 'the story and internal findings pages', 'a finding\'s history', 'the repo\'s findings', 'the verify dispatch')
RESIDUAL_FOLDS = ()   # 0.2.226 §0 (E122.75; operator 2026-11-05: ONE block): EMPTY — the three 0.2.205 §1 residual folds (advances, track-positions, bug-disclosure) retire in this tag with the read door; each word's row home is RESIDUAL_FOLD_HOMES (keyed by CENSUS_GAP_PROJECTIONS' kebab words)
# 0.2.205 §1: cSwap's census GAP projections (sprint-0.7/r2-census/census-door.tsv ddee9a66: the read sites no store table
# answers); each is served by a RESIDUAL_FOLDS word (its kebab spelling)
CENSUS_GAP_PROJECTIONS = ('advances', 'track_positions', 'bug_disclosure')
# 0.2.226 (E122.75; the operator 2026-11-04: "plan the store move, we want to remove dep on ledger, yes complete E31 release 2"):
# R1 completed in ONE block (operator 2026-11-05: "one block store move"; S1–S3 ship together in block 14; rollback = snapshot-restore
# of the stores taken at the seal): every facet read serves its store rows; the editions and campaigns backfills land and write the
# marks (the per-store `marks` table, E124 (64)); E31 release 2's writes move per tenant; the read door (store when backfilled, else
# fold) and RESIDUAL_FOLDS are removed, the fold code deleted, the global campaigns.db deleted once every tenant's copy checks. A route
# whose store has NO mark row answers 503 store_incomplete (codes_vocab) naming the facet; a HELD org (§3a: mark state held {reason})
# keeps the fold until a person clears the hold; a complete org never folds. The ledger is read by exactly LEDGER_READERS: the one-shot
# backfill, the two routes that are ABOUT the ledger (they serve ledger lines by definition) and the boot rules; nothing else opens
# ledger/events.jsonl or live/configevents.jsonl (LEDGER_FILES). §0 hard rule (E124 (72)): every writer writes the store row and the
# ledger line in ONE step (an append-only writer is a defect); the block's B12 carries a census that none remains. E124 (79): for the
# config-ledger writers "one step" = STORE-FIRST ORDER (the row, then the line) with the boot catch-up as the repair.
LEDGER_FILES = ('ledger/events.jsonl', 'live/configevents.jsonl')   # TENANT_FILES paths: the verification ledger and the config ledger
LEDGER_READERS = ('backfill', 'setup.events.list', 'identity.audit.list', 'boot_rules')   # §0: the route words are live ROUTE_SOURCES rows of class ledger whose block N+1 source is still the ledger
# §1: ROUTE_SOURCES, (route, class, source, after_source): what answers each read route TODAY (the census) and what answers it from
# block 14, once the door is gone. Seeded from the census CENSUS-read-routes-store-vs-ledger (conductor b91fbeb0, sha b00888f4: a CODE READ, 70 rows; the
# FINAL's "66" is its count); ROUTE_SOURCES_MEASURED flips to True when cMeasure confirms the table on the running conductor (a
# disagreement is recorded as a row change, not argued). `class` is a ROUTE_SOURCE_CLASSES word: store (facet rows), ledger (a
# LEDGER_FILES file alone), mix (rows or files beside a ledger file), file (the census's "other": org.toml, the config repo, the
# mirror, session or token files, edition files, static copy). `source` is `|`-joined tokens: a `<store>.<table>` (a STORES store
# and a declared table), a TENANT_FILES path, a LEDGER_FILES file, or a plain word (`none`: fixed copy; `credentials`: the token
# files; `catalogue`: the vendored image catalogue; `conductor_config`, `pending_installations`: deployment files outside the
# tenant directory). validate: the class follows from the tokens; an after_source carries a ledger token only on a LEDGER_READERS
# route; a store or file row's after_source adds store tokens at most (§2: a file read may move to rows; nothing new is a file).
# §8 (operator 2026-11-05, defaults): the 38 file rows are OUT of scope (they stay as they are); the two ledger routes stay on the ledger.
ROUTE_SOURCE_CLASSES = ('store', 'ledger', 'file', 'mix')
ROUTE_SOURCES_MEASURED = False   # §1: True once cMeasure's numbers land (a lane, read-only on the real binary)
ROUTE_SOURCES = (
    # (route, class, source today (census b91fbeb0), source from block 14 (§2))
    ('verify.sessions.list',             'file',   'runs/<repo>/sessions/',                                          'runs/<repo>/sessions/'),
    ('verify.sessions.get',              'file',   'runs/<repo>/sessions/',                                          'runs/<repo>/sessions/'),
    ('identity.repos.list',              'file',   'live/org.toml',                                                  'live/org.toml'),
    ('identity.whoami.get@repo',         'file',   'live/org.toml',                                                  'live/org.toml'),
    ('picture.logo.get',                 'file',   '<theme.logo>',                                                   '<theme.logo>'),
    ('verify.runs.list',                 'mix',    'runs.runs | ledger/events.jsonl | books/main/',                  'runs.runs | books/main/'),
    ('verify.runs.get',                  'mix',    'runs.runs | ledger/events.jsonl',                                'runs.runs'),
    ('verify.run_evidence.get',          'mix',    'runs.runs | ledger/events.jsonl | runs/<repo>/artifacts/',       'runs.runs | runs/<repo>/artifacts/'),   # the artifact stays a file (an artifact, not state)
    ('picture.bugs.list',                'mix',    'bugs.bugs | runs.runs | findings.findings | ledger/events.jsonl', 'bugs.bugs | runs.runs | findings.findings'),   # fold_bugs always runs today (the bug-disclosure residual); repair_hold reads the finding row (liveness under_review + repair_state)
    ('picture.bugs.get',                 'mix',    'bugs.bugs | ledger/events.jsonl',                                'bugs.bugs | findings.finding_history'),   # the disclosure trail reads finding_history rows
    ('picture.findings.list',            'mix',    'findings.findings | ledger/events.jsonl | books/main/',          'findings.findings | books/main/'),
    ('picture.findings.get',             'mix',    'findings.findings | ledger/events.jsonl | books/main/',          'findings.findings | books/main/'),
    ('picture.finding_history.get',      'mix',    'findings.finding_history | ledger/events.jsonl',                 'findings.finding_history'),
    ('picture.overview.get',             'mix',    'books/main/ | ledger/events.jsonl | runs.runs',                  'books/main/ | editions.promotions | editions.tracks | runs.runs'),   # the rollback flag from promotions (the advances fold); the position from tracks
    ('picture.metrics.get',              'mix',    'runs.runs | findings.findings | ledger/events.jsonl',            'runs.runs | findings.findings'),
    ('picture.specgraph.get',            'file',   'books/main/',                                                    'books/main/'),
    ('picture.specs.list',               'mix',    'books/main/ | runs.capabilities | ledger/events.jsonl',          'books/main/ | runs.capabilities | editions.editions'),   # the capability set from runs rows only (the instrumentation fold retires); the served set from the edition row
    ('picture.catalogue.get',            'file',   'books/main/',                                                    'books/main/'),
    ('picture.labels.get',               'file',   'none',                                                           'none'),
    ('identity.tokens.list',             'file',   'credentials',                                                    'credentials'),
    ('campaigns.health.get',             'store',  'campaigns.schema_meta',                                          'campaigns.schema_meta'),
    ('campaigns.context.get',            'store',  'campaigns.label_display_map',                                    'campaigns.label_display_map'),
    ('campaigns.cards.list',             'store',  'campaigns.campaigns | campaigns.campaign_observations',          'campaigns.campaigns | campaigns.campaign_observations'),
    ('campaigns.credits.get',            'store',  'credits.credit_pool | campaigns.credit_pool',                    'credits.credit_pool | campaigns.credit_pool'),
    ('conversations.conversations.list', 'store',  'campaigns.conversations',                                        'campaigns.conversations'),
    ('conversations.conversations.get',  'store',  'campaigns.conversations',                                        'campaigns.conversations'),
    ('setup.config.get@repo',            'file',   'live/org.toml | config/',                                        'live/org.toml | config/'),
    ('setup.config.get',                 'file',   'live/org.toml | config/',                                        'live/org.toml | config/'),
    ('setup.repo.get@repo',              'file',   'live/org.toml | config/',                                        'live/org.toml | config/'),
    ('setup.repo.get',                   'file',   'live/org.toml | config/',                                        'live/org.toml | config/'),
    ('setup.repo_refs.get@repo',         'file',   'clones/<repo>/',                                                 'clones/<repo>/'),
    ('setup.repo_refs.get',              'file',   'clones/<repo>/',                                                 'clones/<repo>/'),
    ('setup.repo_preview.get@repo',      'file',   'clones/<repo>/',                                                 'clones/<repo>/'),
    ('setup.repo_preview.get',           'file',   'clones/<repo>/',                                                 'clones/<repo>/'),
    ('setup.image.get@repo',             'file',   'live/org.toml | config/',                                        'editions.images | editions.bakes | live/org.toml | config/'),   # §2 image: the card from the images row (seeded on its first write, E124 (61)) and the bake lines
    ('setup.image.get',                  'file',   'live/org.toml | config/',                                        'editions.images | editions.bakes | live/org.toml | config/'),
    ('identity.whoami.get',              'file',   'live/org.toml',                                                  'live/org.toml'),
    ('setup.overview.get',               'store',  'campaigns.schema_meta | credits.credit_pool | live/org.toml',    'campaigns.schema_meta | credits.credit_pool | live/org.toml'),
    ('strands.inbox.list',               'store',  'strands.strands',                                                'strands.strands'),
    ('strands.inbox_all.list',           'store',  'strands.strands',                                                'strands.strands'),
    ('strands.strand.list',              'store',  'strands.strands',                                                'strands.strands'),
    ('strands.strand.get',               'store',  'strands.strands',                                                'strands.strands'),
    ('identity.staff.whoami',            'file',   'conductor_config',                                               'conductor_config'),   # the deployment's conductor.toml staff roster, no tenant file
    ('tenants.onboard.check',            'file',   'live/org.toml',                                                  'live/org.toml'),
    ('live.org.get',                     'mix',    'live/org.toml | live/configevents.jsonl',                        'live/org.toml | editions.promotions | editions.bakes'),   # limits from org.toml; the history from promotions and bakes; the answer's `head` = max(ledger_seq) over the rows read
    ('tenants.installations_pending.list', 'file', 'pending_installations',                                          'pending_installations'),   # the orgstate pending dir (PENDING_INSTALLATION_SCHEMA), no tenant file
    ('setup.queue.get',                  'ledger', 'live/configevents.jsonl',                                        'editions.tracks | editions.images'),   # derived from rows: tracks.requested_commit IS NOT NULL ∪ images.state = requested; the answer's `head` = max(ledger_seq) over the rows read
    ('setup.events.list',                'ledger', 'live/configevents.jsonl',                                        'live/configevents.jsonl'),   # KEPT: it serves ledger lines by definition (LEDGER_READERS)
    ('setup.request.get',                'file',   'config/',                                                        'config/'),
    ('setup.repo_request.get',           'file',   'config/',                                                        'config/'),
    ('live.repo.get',                    'file',   'live/org.toml | config/',                                        'live/org.toml | editions.tracks'),   # the TrackCard from the tracks row
    ('setup.image_plans.get',            'file',   'catalogue | live/org.toml',                                      'catalogue | live/org.toml'),
    ('live.image.get',                   'store',  'live/org.toml | editions.images | editions.bakes',               'live/org.toml | editions.images | editions.bakes'),   # 0.2.227 §1 (E124 (91)): the pin history from the bakes rows (m10: the entry's fields, the unpinned rows), configevents.jsonl LEAVES the row (the last ledger fold); the keep count from org.toml
    ('identity.seat_tokens.list',        'file',   'credentials',                                                    'credentials'),
    ('setup.repo_features.get',          'ledger', 'ledger/events.jsonl',                                            'live/org.toml | findings.findings'),   # E124 (66), (86): hold = the repo's dt_repair_hold SETTING (org.toml repos[].features, written by setup.repo_features.put) OR any finding of the repo under_review with its repair_state (0.2.221); the latest-event read retires
    ('mirrors.layers.detect',            'file',   'live/org.toml',                                                  'live/org.toml'),
    ('identity.access.explain',          'file',   'live/org.toml',                                                  'live/org.toml'),
    ('live.digests.list',                'file',   'runs/<repo>/sessions/',                                          'runs/<repo>/sessions/'),
    ('campaigns.org_credits.get',        'store',  'credits.credit_pool | campaigns.credit_pool',                    'credits.credit_pool | campaigns.credit_pool'),
    ('campaigns.label_map.get',          'store',  'campaigns.label_display_map',                                    'campaigns.label_display_map'),
    ('identity.own_cli_tokens.list',     'file',   'credentials',                                                    'credentials'),
    ('identity.cli_tokens.list',         'file',   'credentials',                                                    'credentials'),
    ('identity.audit.list',              'ledger', 'ledger/events.jsonl',                                            'ledger/events.jsonl'),   # KEPT: it serves ledger lines by definition (LEDGER_READERS)
    ('identity.own_slack_link.get',      'store',  'identity.aliases',                                               'identity.aliases'),
    ('identity.slack_links.list',        'store',  'identity.aliases',                                               'identity.aliases'),
    ('identity.platform_tokens.list',    'file',   'credentials',                                                    'credentials'),
    ('notify.slack.get',                 'file',   'live/org.toml | config/',                                        'live/org.toml | config/'),
    ('notify.channels.list',             'file',   'live/org.toml',                                                  'live/org.toml'),
    ('notify.binding.get',               'file',   'live/org.toml',                                                  'live/org.toml'),
    ('notify.deliveries.list',           'file',   'observations.log',                                               'observations.log'),
)
# 0.2.233 §1 (R1 completed): a witness proves a change to DURABLE state. An action whose effects fall only on DERIVED data state (a clone, a
# mirror, the materialised books) or that changes nothing (a check) writes no witness BY RULE: UNWITNESSED_BY_RULE (route, reason), reason
# ∈ UNWITNESS_REASONS. 0.2.237 §3: the credential facet's org- and repo-scoped routes (CREDENTIAL_ROUTES, derived below) record the credential
# kinds; its platform-scoped routes (PLATFORM_CREDENTIAL_ROUTES) record nothing, with the reason PLATFORM_CREDENTIAL_REASON. 0.2.234 §2 (E124
# 112): the audit's three pending rows got their kinds and UNWITNESSED_PENDING_ROUTES, empty, is deleted; 0.2.234 §4 (E124 117):
# books.dev_arc.create joins the rule (derived: it materialises an arc a box job authored on the forge).
# validate: every UNWITNESSED_BY_RULE route records nothing; every non-GET live row that records nothing is in UNWITNESSED_BY_RULE or
# PLATFORM_CREDENTIAL_ROUTES (0.2.237 §3: a CREDENTIAL_ROUTES row records its kind).
UNWITNESS_REASONS = ('derived', 'check')
UNWITNESSED_BY_RULE = (
    ('books.global.sync',    'derived'), ('books.trunk.sync',      'derived'), ('mirrors.track.sync',  'derived'),
    ('mirrors.clone.sync',   'derived'), ('mirrors.mirror.sync',   'derived'),
    ('setup.image.check',    'check'),   ('notify.binding.check',  'check'),   ('notify.internal.check', 'check'),
    ('notify.slack.check',   'check'),   # the customer's own check (0.2.226 §6a): a test post, nothing durable changes
    ('books.dev_arc.create', 'derived'),   # 0.2.234 §4 (E124 117): arc.created retires; the arc is a box job's forge write, materialised
)
# 0.2.237 §1 (registry/identity.py CREDENTIAL, the operator 2026-11-03: a credential is a FACET — a store record (state/credentials/
# credential.schema.json) whose word is derived: live until revoked_at or expires_at is set / passed). CREDENTIAL_STATES = the registry's CRD_LIVE,
# CRD_REVOKED, CRD_EXPIRED; CREDENTIAL_TRANSITIONS = the registry's fifteen (IDENTITY_ACTIONS), one row each, (from, action, cause, to) as every
# facet's table (0.2.221): the cause names the route(s) the action is, as MEASURED at LIVE 6e15f706 — identity.tokens.mint / .revoke are the
# ci tokens' repo-plane twins (mint_ci / revoke_ci); identity.ci_tokens.revoke and identity.seat_tokens.revoke revoke ONE token by value
# (revoke_ci / revoke_seat); identity.service_tokens.mint / .revoke are retired (410, no store write) and identity.service_token.revoke revokes a
# platform-scoped service token: none of the three is a cause. `lapse` and `expire` are true clocks (registry EV__TK_CONVERSATION_TOTAL / _IDLE,
# EV__TK_CREDENTIAL_TTL). THREE transitions have NO store write at LIVE (CREDENTIAL_STORELESS_ACTIONS, each a divergence the registry carries):
# purge (the member.removed arm writes principal.revoked and deletes no credential; the App does not subscribe), converse
# (conversations.chat.create mints no credential), lapse (no conversation clock exists): their rows stay as the target, their lines come with
# the mechanisms. validate: a facet table (states, actions snake, every cause a registry row, the webhook's arm or a clock); fifteen rows.
CREDENTIAL_STATES = ('live', 'revoked', 'expired')
CREDENTIAL_TRANSITIONS = (   # (from, action, cause, to)
    ('—',       'signin',          'identity.session.create',                                                    'live'),      # a browser session (GET /auth/callback)
    ('—',       'mint_cli',        'identity.cli_token.mint',                                                    'live'),      # a cli token for the presser
    ('—',       'converse',        'conversations.chat.create',                                                  'live'),      # the conversation credential (auth.md §4.2); storeless at LIVE
    ('—',       'mint_ci',         'identity.ci_tokens.mint | identity.tokens.mint',                             'live'),      # a ci token: the admin row and its repo-plane twin
    ('—',       'mint_seat',       'identity.seat_tokens.mint',                                                  'live'),      # a seat token
    ('—',       'mint_platform',   'identity.platform_tokens.mint',                                              'live'),      # a box or ops token
    ('live',    'signout',         'identity.session.revoke',                                                    'revoked'),   # POST /auth/logout
    ('live',    'revoke_own',      'identity.own_cli_token.revoke',                                              'revoked'),
    ('live',    'revoke_cli',      'identity.cli_token.revoke',                                                  'revoked'),
    ('live',    'revoke_ci',       'identity.ci_token.revoke | identity.ci_tokens.revoke | identity.tokens.revoke', 'revoked'),   # by id; by value; the repo-plane twin
    ('live',    'revoke_seat',     'identity.seat_token.revoke | identity.seat_tokens.revoke',                    'revoked'),   # by id; by value
    ('live',    'revoke_platform', 'identity.platform_token.revoke',                                             'revoked'),
    ('live',    'purge',           'intake.webhook.receive · member.removed',                                    'revoked'),   # every credential of the removed person; storeless at LIVE
    ('live',    'lapse',           'clock.credential.conversation_total | clock.credential.conversation_idle',   'revoked'),   # 12 h total, 1 h idle; conversations only; storeless at LIVE
    ('live',    'expire',          'clock.credential.ttl',                                                       'expired'),   # the kind's ttl, noticed on use and by the hourly sweep
)
CREDENTIAL_STORELESS_ACTIONS = ('converse', 'purge', 'lapse')   # 0.2.237 §1: no store write at LIVE 6e15f706; nothing recorded this cut; divergences
# 0.2.237 §2, §3 (registry datastate ledger_events' divergence): the ledger is per org (tenants/{org}/ledger/events.jsonl); only ORG- and
# REPO-scoped credentials (ci, seat, service, conversation) have a chain. The PLATFORM-scoped credentials — browser sessions, cli tokens,
# platform (box, ops) tokens — belong to no org: their mint and revoke rows record NOTHING this cut, the gap closes when a platform chain
# exists; the KINDS rows admit scope `platform` for that day. identity.service_token.revoke (a platform-scoped service token's; a 410 row
# today) is listed with them. validate: each row is a live registry row, in no other tuple, recording no credential kind.
PLATFORM_CREDENTIAL_ROUTES = ('identity.session.create', 'identity.cli_token.mint', 'identity.platform_tokens.mint',
                              'identity.session.revoke', 'identity.own_cli_token.revoke', 'identity.cli_token.revoke', 'identity.platform_token.revoke',
                              'identity.service_token.revoke')
PLATFORM_CREDENTIAL_REASON = 'platform-scoped records (browser sessions, cli, platform, box and operator tokens) have no chain: every ledger is per org; closes when a platform ledger exists'
# 0.2.237 §3: the routes that RECORD the credential kinds, DERIVED from the table: every route a mint (→ live) or revoke (live → revoked)
# transition names, the storeless actions' and the platform-scoped routes aside. CREDENTIAL_ROUTES (0.2.233 §1) is their union; the
# 0.2.233 §1 coverage rule reads PLATFORM_CREDENTIAL_ROUTES for the rows that still record nothing.
CREDENTIAL_MINT_ROUTES = tuple(_r for _f, _a, _c, _t in CREDENTIAL_TRANSITIONS if _t == 'live' and _a not in CREDENTIAL_STORELESS_ACTIONS
                               for _r in _c.split(' | ') if ' ' not in _r and _r not in PLATFORM_CREDENTIAL_ROUTES)
CREDENTIAL_REVOKE_ROUTES = tuple(_r for _f, _a, _c, _t in CREDENTIAL_TRANSITIONS if _f == 'live' and _t == 'revoked' and _a not in CREDENTIAL_STORELESS_ACTIONS
                                 for _r in _c.split(' | ') if ' ' not in _r and _r not in PLATFORM_CREDENTIAL_ROUTES)
CREDENTIAL_ROUTES = CREDENTIAL_MINT_ROUTES + CREDENTIAL_REVOKE_ROUTES
# 0.2.233 §3: who sets each org.toml key, (key, setter, ref, field): setter ∈ CONFIG_SETTER_WORDS. `route`: ref = the registry row whose body
# carries `field` (the key itself when field is the key; `-` = the row's own action sets the key with no body field: the lock, the accept);
# `provision`, `config_repo`, `hand`: ref = the conductor site at 8017d8c2 (a file:line, or `-`; 0.2.234 §1's two rows at 9e377334). One row per org.schema key, top level
# then repos[]; validate: every key once, no row for a key the schema lacks; a route setter's row is live and its body carries the field.
# 0.2.236 §3 (rule A3; block 19 cert L): a route setter whose write changes nothing writes NO config-ledger witness (campaigns.label_map.put
# re-posting the stored mapping unchanged writes no installation.label line): the config ledger records changes, not requests — the rule of
# every config kind; the handler compares before writing. Prose only.
CONFIG_SETTER_WORDS = ('route', 'provision', 'config_repo', 'hand')
# 0.2.234 §1 (R6, E124 109): the seat_forge_login pattern, OrgToml's at 9e377334 (crates/orgstate/src/live.rs:407-413): 1-39 ASCII letters,
# digits, `-`, `_` or `.`, starting and ending with a letter or digit. validate: org.schema's pattern is this string.
SEAT_FORGE_LOGIN_PATTERN = '^[A-Za-z0-9]([A-Za-z0-9._-]{0,37}[A-Za-z0-9])?$'
CONFIG_SETTERS = (
    ('slug',                  'route',       'live.platform.put',         'slug'),
    ('github_owner',          'route',       'live.platform.put',         'github_owner'),
    ('edition_owner',         'hand',        '-',                         ''),   # no provision write at 8017d8c2 (crates/orgstate/src/live.rs:2734 repairs a missing line at boot)
    ('installation_state',    'route',       'live.platform.put',         'installation_state'),   # the App webhook also writes it (crates/routes/src/hooks.rs:3898)
    ('installation_id',       'route',       'live.platform.put',         'installation_id'),
    ('jobs',                  'hand',        '-',                         ''),   # no provision or route write at 8017d8c2
    ('limits',                'route',       'live.platform.put',         'limits'),   # pricing.limits.put writes two of its keys (PricingLimitsBody)
    ('internal_default_line', 'route',       'live.platform.put',         'internal_default_line'),
    ('onboarding',            'route',       'live.platform.put',         'onboarding'),
    ('notify',                'route',       'notify.slack.put',          'channel_id'),   # the binding: channel_id, channel_name, events, enabled (NotifyBindingRequest); installation_ref by the install callback
    ('switches',              'route',       'live.platform.put',         'switches'),
    ('whitelist',             'route',       'live.platform.put',         'whitelist'),
    ('access',                'route',       'live.platform.put',         'access'),
    ('private_upstream',      'route',       'live.platform.put',         'private_upstream'),
    ('credit_rate_per_hour',  'route',       'live.platform.put',         'credit_rate_per_hour'),
    ('profile',               'route',       'live.platform.put',         'profile'),
    ('theme',                 'route',       'live.platform.put',         'theme'),   # live.logo.put writes theme.logo (no body key)
    ('repos',                 'route',       'live.platform.put',         'repos'),
    ('images',                'route',       'live.platform.put',         'images'),   # the built route moves the pin (image.built)
    ('internal',              'provision',   'crates/orgstate/src/provision.rs:1220', ''),
    ('aretta_owned',          'provision',   'crates/orgstate/src/live.rs:1419',      ''),   # the identity apply at provision
    ('seat',                  'provision',   '-',                         ''),   # the provision mints the seat (no key write found at 8017d8c2)
    ('seat_forge_login',      'provision',   '-',                         ''),   # 0.2.234 §1 (R6): the seat's forge identity; no writer at 9e377334 (crates/orgstate/src/live.rs:148: read only this release)
    ('seat_forge_email',      'provision',   '-',                         ''),   # 0.2.234 §1 (R6): with seat_forge_login, both or neither
    ('repos[].name',          'route',       'live.platform.put',         'repos'),
    ('repos[].sut',           'provision',   'crates/routes/src/hooks.rs:570', ''),   # the installation's repos (upstream::resolve_with_layout)
    ('repos[].mirror',        'provision',   '-',                         ''),   # the box tenant route creates the mirror
    ('repos[].github_repo',   'provision',   'crates/orgstate/src/live.rs:1620', ''),
    ('repos[].upstream',      'provision',   'crates/auth/src/access_facts.rs:463', ''),   # read-side only at 8017d8c2
    ('repos[].track',         'config_repo', 'crates/routes/src/adminwrites.rs:374', ''),   # the accept applies repos/<repo>.toml (RepoBody.track)
    ('repos[].advance',       'config_repo', '-',                         ''),   # repos/<repo>.toml (RepoBody.advance)
    ('repos[].advance_locked','route',       'live.track_state.lock',     '-'),   # the lock route's own action, no body field (crates/orgstate/src/apply.rs:1019); live.track_state.unlock clears it
    ('repos[].accepted_commit','config_repo','crates/routes/src/adminwrites.rs:374', ''),   # the accept
    ('repos[].chain_aliases', 'hand',        '-',                         ''),   # crates/orgstate/src/live.rs:1366 constructs it empty; no writer
    ('repos[].features',      'route',       'setup.repo_features.put',   'dt_repair'),   # RepoFeaturesBody.dt_repair -> features.dt_repair_hold (0.2.226 E124 (86))
)
KIND_WRITERS = (   # (kind, writer); 0.2.233 §4; a name both ledgers carry (track.reset) is one row
    ('org.imported',                    'ingest'),
    ('run.stamped',                     'box.callback'),
    ('run.imported',                    'ingest'),
    ('run.amended',                     'verify.run.amend'),
    ('bug.opened',                      'ingest'),
    ('bug.evidence',                    'ingest'),
    ('bug.verdict',                     'findings.bug.dispose'),
    ('disclosure.recorded',             'findings.finding.disclose'),
    ('disclosure.withheld',             'retired'),
    ('org.publication_hold',            'findings.publication.hold'),
    ('edition.advanced',                'books.edition.promote'),
    ('book.completed',                  'books.fork.sync'),
    ('finding.curated',                 'findings.finding.curate'),
    ('finding.revised',                 'findings.finding.retract'),
    ('finding.issue_linked',            'retired'),
    ('finding.filed',                   'retired'),
    ('fix.candidate',                   'ingest'),
    ('finding.disclosure_approved',     'retired'),
    ('finding.followup_published',      'publisher'),
    ('finding.healed',                  'ingest'),
    ('finding.repo_attributed',         'ingest'),
    ('finding.reverify_incomplete',     'box.callback'),
    ('finding.reverify_failed',         'findings.repair.reject'),
    ('finding.proof_repair_pending',    'findings.triage.verdict'),
    ('witness.minted',                  'retired'),
    ('witness.revoked',                 'retired'),
    ('report.published',                'ingest'),
    ('monitor.observed',                'ingest'),
    ('instrumentation.observed',        'mirrors.accessor_prs.sweep'),
    ('bug.forwarded',                   'ingest'),
    ('queue.record_merged',             'clock'),
    ('queue.record_skipped',            'clock'),
    ('dt.build_gate_filed',             'ingest'),
    ('triage.filed',                    'ingest'),
    ('dt.repair_hold',                  'retired'),
    ('contribution.offered',            'findings.upstream_pull.publish'),
    ('heal.redriven',                   'ingest'),
    ('heal.red_surfaced',               'ingest'),
    ('serving.unmeasured_alarmed',      'clock'),
    ('edition.cut',                     'clock'),
    ('pipeline.step_fired',             'clock'),
    ('track.upstream_moved',            'ingest'),
    ('run.battery_completed',           'box.callback'),
    ('finding.fix_landed',              'retired'),
    ('heal.redrive_requested',          'findings.heal.redrive'),   # 0.2.238 §2: the submit seam retired; the one live route recording it (heal.rs:5039 heal_redrive at 2312529f)
    ('ledger.compacted',                'ops'),
    ('bug.refused',                     'ingest'),
    ('track.sync_requested',            'mirrors.track.request_sync'),
    ('track.reset',                     'mirrors.track.reset'),
    ('notify.delivered',                'clock'),
    ('notify.blocked',                  'clock'),
    ('slack.mention',                   'ingest'),
    ('publish.blocked',                 'retired'),   # 0.2.236 §2: a RETIRED_KINDS row — both writers flipped (at 6e15f706 heal.rs:2849 and publisher_issues.rs:1625 still stamped it; block 20 flips both)
    ('finding.defer',                   'box.callback'),   # 0.2.235 §3 / 0.2.236 §2: the heal's hold gate (heal.rs:2821 deliver_or_defer at 6e15f706, the settle of a box re-verify); flips in block 20 (the floor knows the kind: kinds.rs:467)
    ('finding.withhold',                'publisher'),   # 0.2.235 §3 / 0.2.236 §2: the publisher's terminal-4xx back-off (publisher_issues.rs:1599 record_publish_block at 6e15f706); the writer flips in block 20 (the floor reads both)
    ('notify.interactive_enabled',      'notify.slack.put'),
    ('notify.interactive_disabled',     'notify.slack.put'),
    ('slack.answered',                  'ingest'),
    ('slack.pressed',                   'notify.interactions.receive'),
    ('slack.declined',                  'ingest'),
    ('slack.reply',                     'ingest'),
    ('principal.revoked',               'clock'),
    ('slack.linked',                    'identity.slack_link.receive'),
    ('slack.unlinked',                  'identity.own_slack_link.revoke'),
    ('person.renamed',                  'identity.own_profile.put'),
    ('credit.moved',                    'campaigns.card.launch'),
    ('job.cost_recorded',               'box.callback'),
    ('strand.proposed',                 'campaigns.card.author'),
    ('strand.revised',                  'strands.strand.revise'),
    ('strand.commented',                'campaigns.card.feedback'),
    ('strand.accepted',                 'findings.triage.verdict'),
    ('strand.rejected',                 'strands.strand.reject'),
    ('strand.deferred',                 'strands.strand.defer'),
    ('strand.act_failed',               'retired'),
    ('strand.withdrawn',                'strands.strand.withdraw'),
    ('strand.invalidated',              'retired'),
    ('strand.revalidated',              'retired'),
    ('strand.retired',                  'campaigns.card.withdraw'),
    ('campaign.authored',               'campaigns.card.author'),
    ('campaign.finalized',              'retired'),
    ('campaign.declined',               'retired'),
    ('campaign.staged',                 'retired'),
    ('campaign.withdrawn',              'campaigns.card.withdraw'),
    ('campaign.launching',              'campaigns.card.launch'),
    ('campaign.launched',               'campaigns.card.launch'),
    ('campaign.retired',                'campaigns.campaigns.retire'),
    ('campaign.completed',              'box.callback'),
    ('campaign.reproposed',             'campaigns.card.repropose'),
    ('campaign.launch_failed',          'box.callback'),
    ('campaign.feedback',               'campaigns.card.feedback'),
    ('strand.ended',                    'strands.strand.end'),
    ('edition.proposed',                'clock'),
    ('edition.dropped',                 'clock'),
    ('finding.feedback',                'strands.strand.comment'),
    ('triage.requested',                'ingest'),
    ('run.requested',                   'verify.sessions.start'),
    ('run.started',                     'verify.box_job.start'),   # 0.2.234 §7: the box's start callback records it (was the in-process word box.callback)
    ('run.held',                        'box.callback'),
    ('run.cancelled',                   'verify.sessions.cancel'),
    ('strand.accept_refused',           'strands.strand.accept'),
    ('org.retired',                     'tenants.org.retire'),
    ('arc.created',                     'retired'),   # 0.2.234 §4 (E124 117): retired; the dev arc is derived
    ('capability.observed',             'mirrors.capabilities.fold'),
    ('canon.matched',                   'picture.canon.match'),
    ('track.requested',                 'setup.repo.put'),
    ('track.accepted',                  'clock'),
    ('track.locked',                    'live.track_state.lock'),
    ('track.unlocked',                  'live.track_state.unlock'),
    ('image.requested',                 'setup.image.put'),
    ('image.accepted',                  'live.image.accept'),
    ('image.built',                     'live.image.report'),
    ('image.unpinned',                  'live.image.unpin'),
    ('onboarding.completed',            'setup.config.complete'),
    ('repo.removed',                    'live.platform.put'),
    ('notify.requested',                'notify.slack.put'),
    ('notify.bound',                    'notify.slack.put'),
    ('notify.unbound',                  'notify.events.receive'),
    ('installation.changed',            'ingest'),
    ('image.dispatched',                'clock'),
    ('pricing.changed',                 'pricing.rate.put'),
    ('clock.paused',                    'clock.pause.put'),
    ('repo.features_set',               'setup.repo_features.put'),
    ('platform.changed',                'live.platform.put'),
    ('contacts.changed',                'setup.contacts.put'),
    ('theme.changed',                   'live.logo.put'),
    ('installation.label',              'campaigns.label_map.put'),   # 0.2.234 §2 (E124 112): the label map's witness (the registry's setup.label_map.put)
    # 0.2.237 §3 (registry EV__CREDENTIAL_MINTED / _REVOKED / _EXPIRED): the mint and revoke rows by the §4 rule (the first CREDENTIAL_ROUTES row
    # by registry order); credential.expired → clock, written at BOTH sites that notice an expiry at 6e15f706 — the lazy check on use
    # (crates/auth/src/store.rs:753 lookup_credential, is_live false → delete) and the hourly ttl sweep (crates/routes/src/login.rs:1228
    # spawn_auth_session_sweep → store.rs:1295 purge_expired) — for org- and repo-scoped credentials only (ci, seat)
    ('credential.minted',               'identity.tokens.mint'),   # 0.2.237 §3
    ('credential.revoked',              'identity.tokens.revoke'),   # 0.2.237 §3
    ('credential.expired',              'clock'),   # 0.2.237 §3: the lazy check on use and the hourly sweep
)
# §0: where each retired residual fold's rows live from block 14, (fold word, store.table[.column]): the advances fold becomes the
# editions promotions table (m6), the track positions the tracks rows (m3/m5), the bug disclosure a bugs column (m5). validate: the
# keys are exactly CENSUS_GAP_PROJECTIONS' kebab words (0.2.205 §1's three GAP projections; RESIDUAL_FOLDS is empty); each home is a
# declared table (and column) of a STORES_LIVE store.
RESIDUAL_FOLD_HOMES = (('advances', 'editions.promotions'), ('track-positions', 'editions.tracks'), ('bug-disclosure', 'bugs.bugs.disclosure_status'))
# §3, §3a (E124 (63), (64)): one one-shot backfill per facet store fills its rows from history and writes the store's MARK: one row
# per (org, store) in that store's `marks` table — {store, state ∈ BACKFILL_MARK_STATES (complete | held), reason ∈ BACKFILL_HOLD_REASONS
# (NULL when complete), since} (BACKFILL_MARK_COLUMNS; the file is the org). It REPLACES the schema_meta `backfill.<table>`
# {completed_at, through_seq} marks of 0.2.179 §5 (STORE_META_KEYS keeps the key one release, read never written: tolerate-first;
# 0.2.180's `backfill.waiting_aliases` deferral reads as held {waiting_aliases}). A read serves rows; an org with NO mark row answers
# 503 store_incomplete; a HELD org keeps the fold and is listed (BACKFILL_HELD_FIELDS: the admin plane's listing row, (org, store, reason,
# since)) with a staff notice (postable notify.backfill_held) until a person clears the hold; the deploy is not held by a held
# backfill. E124 (72): a provisioned org's or a late org.toml's marks come from RUNNING the real boot_one backfill (the edition files
# included), never from an empty-chain "complete". The marks were measured 2026-11-05: runs / findings / bugs complete on all nine
# tenants (turso's two malformed findings pins F-035 / F-036 are investigated one by one before any data fix, §8). BACKFILL_SOURCES:
# (store, what the backfill reads, the mark it writes). runs, findings and bugs are 0.2.179's backfills (BACKFILL_RULES); findings
# also reads the served edition files (E124 (76): a finding no edition.advanced receipt names is read from books/main/); editions
# (promotions, bakes, tracks, images) is new; campaigns is §4a's copy of the global file (E31 release 2; see its row). Emitted to
# vocabulary.rs as BACKFILL_STORE_SOURCES (BACKFILL_SOURCES there is 0.2.179 §4's per-table kinds). validate: every store in
# STORES_LIVE, the fold-retirement stores + editions + campaigns, each declaring a `marks` table with BACKFILL_MARK_COLUMNS; its source
# LEDGER_FILES files, TENANT_FILES paths or the global campaigns file.
BACKFILL_MARK_STATES = ('complete', 'held')   # <store>.db marks.state
BACKFILL_HOLD_REASONS = ('waiting_aliases', 'unreadable_history', 'checksum_mismatch', 'gaps', 'serial_collision')   # <store>.db marks.reason (E124 (78), final): a deferred alias (0.2.180), unreadable or gapped history (§3a), §4a's per-org content checksum check, a serial collision on the §4a merge (a person re-serials)
BACKFILL_MARK_COLUMNS = (('state', 'BACKFILL_MARK_STATES'), ('reason', 'BACKFILL_HOLD_REASONS'), ('since', 'time'),   # beside the key `store`
                         ('detail', 'string'))   # 0.2.230 §2 (E124 (85)): NULL on every row except held {serial_collision}, where it carries the colliding id and both org ids in one phrase; runs m6, findings m10, bugs m7, editions m11, campaigns m29 (additive)
BACKFILL_HELD_FIELDS = ('org', 'store', 'reason', 'since', 'detail')   # 0.2.230 §2 (E124 (85)): + detail (the mark's phrase, NULL unless a serial collision); §3a: the admin plane's listing of held orgs, one row per held (org, store); the carrier is prose (no new row: §6)
BACKFILL_SOURCES = (   # (store, source, mark)
    ('runs',      'ledger/events.jsonl',                              'marks'),
    ('findings',  'ledger/events.jsonl | books/main/',                'marks'),   # E124 (76): the served edition files, read when no edition.advanced receipt names the finding
    ('bugs',      'ledger/events.jsonl',                              'marks'),
    ('editions',  'live/configevents.jsonl | ledger/events.jsonl',    'marks'),   # promotions and the edition rows from edition.advanced (events), bakes / tracks / images from the config ledger
    # §4a, §4b (E124 (63), (69), (73), (74), (78)): the copy of every EXISTING campaign and conversation row of the org from the global
    # file into the tenant's store. The copy records a per-org CONTENT CHECKSUM of the global rows; while the global file exists a later
    # boot MERGES BY ID into the tenant store only when that checksum changed; on an id conflict the TENANT row wins and the conflict is
    # logged into the held notice's detail (never a write back); the post-merge check is tenant ⊇ global by id (not equality) under the
    # same checksum; a persisting difference holds the tenant {checksum_mismatch}; a serial collision holds it {serial_collision} and a
    # person re-serials. An orphan slug (rows with no tenant directory) gets a per-tenant campaigns store on demand at its first write.
    # After the flip no head writer (boot rules, adopt, migrate) writes the global file. §4b (the door, 2026-11-05): block 14 does NOT
    # delete the global campaigns.db — it stays read-only to the head (a floor rollback would otherwise re-derive every tenant store to
    # empty) and the merge keeps running; the deletion (S3's last step) moves to a later block, once a floor that reads the tenant stores
    # exists. No new CAMPAIGNS_WRITE_TRUTH word.
    ('campaigns', 'campaigns/campaigns.db',                           'marks'),
    # 0.2.231 §1 (E124 (97b)): a COLUMN FILL, not a store backfill (mark word `fill`): at boot, every bakes row whose track_ref, artifact_version
    # and note are NULL and whose image.built line (by ledger_seq) carries them is filled from the line; idempotent (NULL-only), re-run at
    # every boot until no such row remains; no mark change (the editions mark stays complete), no 503. Block 14's rows were written before
    # m11 and nothing re-read their lines (the block 15 cert: 7 orgs' notes unserved).
    ('editions.bakes.columns', 'live/configevents.jsonl',                 'fill'),
)
# THE VOCABULARY, once: the words the stores' enum CHECK constraints carry (state/stores/vocabulary.json is generated from
# these; the Rust and TypeScript lists too). A word the spec does not know cannot be stored.
# 0.2.173 §2 (one meaning per word): the thing a strand is about is a TOPIC (was SUBJECT_KINDS; `subject` is a principal's id)
TOPIC_KINDS = ('finding', 'bug', 'pull', 'campaign', 'run', 'conversation', 'edition', 'track', 'triage', 'image')   # 0.2.222 §2: image = the org's image request (org + kind), Aretta-internal   # 0.2.221 §2: triage = a red heal's uncertain verdict, Aretta-internal   # conversation = a STUB row until 0.7b; edition, track (0.2.176): a candidate edition, a repo's track
STRAND_KINDS = ('finding', 'pull', 'campaign', 'edition_promote', 'track_move',   # 0.2.176: the two internal kinds (STRAND_KIND_AUDIENCE)
                'conversation',   # 0.2.182 §1: a chat; its own topic, customer audience, no target (nothing to accept)
                'track',          # 0.2.222 §1 (E122.67): the customer's strand on a track request (topic track); no accept press: the battery accepts (closes track_accepted)
                'image',          # 0.2.222 §2 (E122.68): the Aretta-internal strand on an image request (topic image); its accept button IS the accept (a bake is heavy); before triage so the accept row's derived records keep the strand kind last (R4)
                'triage')         # 0.2.221 §2 (E122.60): the staff triage strand on an uncertain verdict; internal audience; accept = the person's verdict
STRAND_ACTIONS = ('propose', 'revise', 'withdraw', 'retire',     # Aretta
                  'accept', 'reject', 'defer', 'comment',        # customer (campaign: accept = launch; reject skip = skip; defer = ask later)
                  'invalidate', 'revalidate', 'expire',          # system
                  'end')                                         # 0.2.182 §1: customer; conversation only; the strand -> retired, reason END_REASON
# 0.2.220 §2 (E122.32): a strand is open or closed; why it closed is closed_reason (STRAND_CLOSED_REASON_WORDS). The old words
# stay in the store's CHECK for one release (tolerate-first, §10.1: read, never written once the ceilings lift) and are mapped on
# read by STRAND_STATE_READ_MAP (§8.4); STRAND_STORED_STATES is the CHECK's set.
STRAND_STATES = ('open', 'closed')
STRAND_STATES_RETIRED = ('proposed', 'discussing', 'accepted', 'released', 'rejected', 'deferred', 'skipped', 'act_failed',
                         'invalidated', 'withdrawn', 'retired', 'superseded')
STRAND_STORED_STATES = STRAND_STATES + STRAND_STATES_RETIRED
STRAND_STATE_READ_MAP = {   # 0.2.220 §8.4: old stored word -> (state, closed_reason or None)
    'proposed': ('open', None), 'discussing': ('open', None), 'deferred': ('open', None), 'act_failed': ('open', None),
    'invalidated': ('open', None),
    'accepted': ('closed', 'accepted'), 'released': ('closed', 'accepted'),
    'rejected': ('closed', 'rejected'), 'skipped': ('closed', 'rejected'),
    'withdrawn': ('closed', 'withdrawn'),
    'retired': ('closed', 'retired'), 'superseded': ('closed', 'retired'),
}
STRAND_TERMINAL = ('closed',)
# 0.2.180 §1 (the operator, DECISIONS 2026-10-26T00:53:06Z): a customer may comment in every state but withdrawn and retired.
# A comment in a state outside COMMENT_MOVES appends its actions row and its strand.commented record (naming the strand's
# current version) and leaves state, version and every pending_presses row as they were: nothing reopens, no press returns,
# a hold stays a hold. In COMMENT_CLOSED_STATES the comment is refused strand_closed (S9), `detail` = the state.
# 0.2.220 §2 (E122.32): the strand's stored words, its closed reasons and the messages it takes while open; one open strand per
# SUBJECT is a precondition on propose (409 strand_open), never a state. A strand retires with its RETIRE_REASON when the topic's
# facet event is written (campaign withdrawn / retired, finding retracted, PR closed unmerged): the facet event's second action.
STRAND_CLOSED_REASONS = ('accepted', 'rejected', 'withdrawn', 'retired', 'ended')   # + RETIRE_REASONS = STRAND_CLOSED_REASON_WORDS (strands.db strands.closed_reason)
STRAND_MESSAGES = ('comment', 'revise', 'defer', 'accept', 'reject')                 # the actions while open; revise = a new version
COMMENT_CLOSED_STATES = ('closed',)        # ⊂ STRAND_TERMINAL; 0.2.220 §2: a comment is a message while open
COMMENT_MOVES = {}   # 0.2.220 §2: a comment moves no state (open stays open)
COMMENTABLE = tuple(s for s in STRAND_STATES if s not in COMMENT_CLOSED_STATES)   # derived, never listed by hand
# 0.2.171 §16 (the reading confirmed, 0.2.168 §7): for a FINDING strand, `target_gone` = the finding is absent from the repo's
# findings (a RETRACTED finding is not invalidated but RETIRED by the system, permanently), and `already_done` = an issue is
# already linked or the finding is healed. For a PULL strand, `target_gone` = the upstream PR is closed unmerged (the strand is
# then retired by the system), and `already_done` = merged.
INVALIDATION_REASONS = ('sut_moved', 'already_done', 'target_gone')
# APPROVED by the operator (dashboard-team's window, 2026-10-23T21:42:18Z; WORDING-SHEET A32–A35): key = the sheet's
# ui.strand.reason_* suffix, word verbatim; a campaign's `skip` is the reject row's reason word, not a reason code.
# 0.2.168 (the door's ruling): a campaign's skip is the reject row with reason `skip`; the word "Skip" is approved (dashboard
# src/copy.ts row A27, 2026-10-26); the four approved reasons are unchanged.
REJECT_REASONS = (('not_a_bug', 'Not a bug'), ('wont_fix', "Won't fix"), ('duplicate', 'Already known'), ('closed', 'Not wanted upstream'),
                  ('skip', 'Skip'),
                  ('hold', 'Hold this edition'), ('stay', 'Stay on this commit'))   # 0.2.176 §7: APPROVED by the operator, 2026-10-25
REJECT_REASONS_BY_KIND = {'finding': ('not_a_bug', 'wont_fix', 'duplicate'), 'pull': ('closed',), 'campaign': ('skip',),
                          'edition_promote': ('hold',), 'track_move': ('stay',)}   # 0.2.176 §4: hold is terminal for this candidate
# 0.2.168 (strands, §3): the closed lists per strand kind, as data. (strand kind, the target registry row an accept runs,
# the target's word): the accept and reject rows' door is `read`; the handler passes the TARGET row's check through the
# one decision function (`confirms: target`) before it runs the act, as the presser. 0.2.182 §1: the rows cover exactly the
# kinds whose STRAND_CUSTOMER_ACTIONS include `accept`, in STRAND_KINDS order (conversation has none: an accept on a kind with
# no target -> 409 strand_closed, detail no_target; WireStrand.target is absent only for such a kind).
# 0.2.220 §2 (E122.51, E122.35/46): (strand kind, the target registry row): the accept request causes BOTH strand::accept and this
# row's action (R4: admitted by the stricter, refused WHOLE with the stricter's code; nothing written on a refusal). The third
# column (the target's word) goes: the door is the target row's own. Every target admits read · repo but the campaign's launch
# (launch_campaign: an accept by a holder without it is refused 403 with nothing written in either store).
STRAND_TARGETS = (
    ('finding',  'findings.finding.disclose'),   # 0.2.221 §4 (E122.60): replaces findings.issue.create
    ('pull',     'findings.upstream_pull.publish'),
    ('campaign', 'campaigns.card.launch'),
    ('edition_promote', 'books.edition.promote'),
    ('track_move',      'mirrors.track.reset'),
    ('image',           'live.image.accept'),        # 0.2.222 §2 (E122.68): the image strand's accept is the human accept of the image request
    ('triage',          'findings.triage.verdict'),   # 0.2.221 §4 (E122.60): the staff triage strand's accept is the person's verdict
)
# 0.2.183 (the ruling of 2026-10-26 on W7; actions.md §4: "a finalize opens the campaign's strand"): the route ids whose action
# opens a strand, read from the registry at v0.2.182 (every row whose records carry strand.proposed). validate, both ways:
# each row's records carry strand.proposed, and every row whose records carry it is one of these.
# 0.2.220 §2 (E122.34): (the event that opens a strand, the strand kind it opens; `*` = any): a registry row's request or a system
# event; the same event also causes the facet's action (R4: the target facet first, then the strand).
STRAND_OPENERS = (
    ('strands.strand.propose', '*'), ('campaigns.card.author', 'campaign'), ('campaigns.card.repropose', 'campaign'),
    ('findings.finding.curate | intake.forge.receive · label.publish', 'finding'), ('notify.events.receive', 'conversation'),   # 0.2.221 §4: curate opens the customer's strand (was disclose)
    ('box.job.finished · triage.uncertain', 'triage'),   # 0.2.221 §4: the staff triage strand (Aretta-internal)
    ('setup.repo.put | setup.repo.put@repo', 'track'),    # 0.2.222 §1: a track request (a config commit) opens the customer's track strand; the same event causes run::start (the battery)
    ('setup.image.put | setup.image.put@repo | live.image.unpin', 'image'),  # 0.2.222 §2: an image request opens the Aretta-internal image strand (the accept button); E124 (36) O4: an unpin re-opens it (0.2.225 §2, E124 (58): live.image.unpin records strand.proposed)
    ('run.battery_completed', 'track_move'), ('the candidate run completes with customer_visible_change', 'edition_promote'))
# 0.2.189 §2 (E48, E54, E56): the system producers, as data: (producing record kind, the strand kind it opens); a person's
# propose is not here. THE CATCH-UP (E48): at boot the head opens every produced strand missing against the chain's producing
# records, idempotent, one strand per producing record's key, reading from the strands store's catchup.seq (STORE_META_KEYS):
# no new store, no new key. validate: each STRAND_OPENERS row appends a STRAND_PRODUCERS kind, or is the doorless mention path
# (`auth:`; its producer is the mention), or records the opening kind alone (a direct propose); a row appending a producer
# kind is a STRAND_OPENERS row.
STRAND_PRODUCERS = (
    ('finding.curated',      'finding'),   # 0.2.221 §2: curate opens the finding strand (disclosure.recorded moves to disclose)
    ('campaign.authored',    'campaign'),   # 0.2.220 §1: author (create + finalize) opens the card's strand
    ('campaign.reproposed',  'campaign'),   # 0.2.220 §1: repropose opens a NEW strand
    ('track.requested',      'track'),      # 0.2.222 §1: a config-ledger kind (not a chain KINDS row): the catch-up reads the config ledger for it
    ('image.requested',      'image'),      # 0.2.222 §2: a config-ledger kind, as above
    ('image.unpinned',       'image'),      # 0.2.225 §2 (E124 (58); E124 (36) O4): the unpin re-opens the image strand; a config-ledger kind (the catch-up reads it; idempotent under the one-open-strand rule)
)
STRAND_REPO_READ = 'repo_or_org'      # a repo-plane strand route matches the org's strands whose topic has that repo OR has no repo
TOPIC_REF_SOURCE = {'edition': 'topic.id', 'track': 'basis.to_commit', 'image': 'basis.request_commit'}   # the wire's topic.ref per topic kind (0.2.181 §1 corrected); 0.2.222 §2: an image topic's ref is the request commit
STRAND_CUSTOMER_ACTIONS = {           # closed per kind; every word is a strands route verb (campaign: accept = launch, defer = ask later)
    'finding':  ('accept', 'reject', 'comment'),
    'pull':     ('accept', 'reject', 'comment'),
    'campaign': ('accept', 'reject', 'defer', 'comment'),
    'edition_promote': ('accept', 'reject', 'comment'),   # 0.2.176 §4
    'track_move':      ('accept', 'reject', 'comment'),
    'conversation':    ('comment', 'end'),   # 0.2.182 §1: a message is a comment; the customer ends it; nothing to accept
    'triage':          ('accept', 'comment'),   # 0.2.221 §2: the accept is the verdict (findings.triage.verdict/<word>)
    'track':           ('defer', 'comment'),    # 0.2.222 §1 (E124 (37) 4): no accept or reject press: the battery accepts (strand::expire, track_accepted)
    'image':           ('accept', 'defer', 'comment'),   # 0.2.222 §2: no reject (a declined request is a withdraw by Aretta); defer admitted (INTERNAL_DEFER_KINDS)
}
# 0.2.222 §2: the internal kinds whose strand admits `defer` (the bake waits on a person); every other internal kind has no defer
# button (0.2.211 §2). validate: exactly these internal kinds list defer.
INTERNAL_DEFER_KINDS = ('image',)
# 0.2.176 §3: who is asked to decide. An internal strand shows only to holders of `steer`, posts to the deployment's internal
# channels naming its org, repo and edition, and is listed across tenants (strands.inbox_all.list); its accept runs its
# target as the presser. A kind's audience flips outward by this one word when Aretta can state the effect and price.
STRAND_AUDIENCES = ('customer', 'internal')      # who is asked to decide; the words the POSTABLE rows already use
STRAND_KIND_AUDIENCE = {'finding': 'customer', 'pull': 'customer', 'campaign': 'customer',
                        'edition_promote': 'internal', 'track_move': 'internal',
                        'conversation': 'customer', 'triage': 'internal',
                        'track': 'customer', 'image': 'internal'}   # 0.2.222 §1/§2   # a kind's audience; flips outward by a later cut (0.2.182: conversation)
STRAND_KIND_TOPIC = {'finding': 'finding', 'pull': 'pull', 'campaign': 'campaign', 'edition_promote': 'edition', 'track_move': 'track',
                     'conversation': 'conversation', 'triage': 'triage', 'track': 'track', 'image': 'image'}   # 0.2.222: the track strand is about the repo's track, the image strand about the org's image   # 0.2.182: a conversation is its own topic   # the topic a strand kind is about; replaces 0.2.165's STRAND_KINDS ⊂ TOPIC_KINDS (G9)
# 0.2.184 §3 (E44): BASIS_SHAPES describes the WIRE: WireStrand.basis's keys per kind, served by one projection of the
# stored strands.db actions.basis_json (which keeps the store's own keys); no store change.
# 0.2.181 §1 (dashboard-team's edition-pipeline read): WireStrand.basis, the current version's basis (the propose or revise
# act's basis_json, 0.2.174), keyed by strand kind; every value is an id, a code or a listed word, never prose. guards items are
# {guard: a PROMOTE_GUARDS word, passed: boolean, detail: object}. WireStrand.schema.json holds one allOf if-kind-then branch per
# kind with exactly these keys (additionalProperties false); validate holds the branches equal to this map.
BASIS_SHAPES = {
    'edition_promote': ('candidate_run', 'served_run', 'customer_visible_change', 'guards'),
    'track_move':      ('from_commit', 'to_commit', 'battery_run'),
    'finding':         ('finding_state',),            # the topic's state the proposal rests on (0.2.168 §5)
    'pull':            ('pull_state',),
    'campaign':        ('campaign_state',),
    'triage':          ('finding_id', 'fixed_rev', 'round'),   # 0.2.221 §2: the triage it decides (TRIAGE_FILED_IDEMPOTENCY_KEYS)
    'track':           ('track', 'to_commit'),        # 0.2.222 §1: the requested track as written and the commit it resolves to (topic.ref, TOPIC_REF_SOURCE)
    'image':           ('request_commit',),           # 0.2.222 §2 (E124 (37) 3): the request only; the bake line is served on ImageView, never here
    'conversation':    ('repos', 'books_edition'),   # 0.2.182 §1: the pins, {repos: {name: commit}, books_edition} (DESIGN-0.7b §2)
}
STRAND_ARETTA_ACTIONS = ('propose', 'revise', 'withdraw', 'retire')   # under `steer`
RESPONDERS = ('fde', 'agent')         # 0.7a: fde for every kind; 0.7b adds agent
# 0.2.168: the registry's `confirms` column (a strands accept or reject row: its door is `read` and its act passes the
# TARGET row's check, STRAND_TARGETS). Closed; validate refuses a word no row uses.
CONFIRMS = ('target',)
FRONTS = ('dashboard', 'slack')                   # + 'linear' later: one row, one delivery function
RETENTION_CLASSES = ('keep', 'customer_text', 'transient',   # comments `keep` by default (the operator, 2026-10-24)
                     'redacted')   # 0.2.182 §1: the act stays, its text removed at the customer's request (strands.db CHECK redacted_has_no_text)
RUN_LINES = ('main', 'dev')
# 0.2.213 §1 (arc-verify slice 1, the box-side namespace): an arc's ref on the SUT mirror is ARC_REF_PREFIX + slug. A ref the box
# is asked to create or push to parses as exactly one of FORK_REFS: Dev (the one internal line, RUN_LINES' `dev`; never a push onto
# it through the arc path, 409 arc_on_dev) | Arc(slug) (the ref starts with ARC_REF_PREFIX and the rest is
# ARC_SLUG_MIN_LENGTH..=ARC_SLUG_MAX_LENGTH characters, each in ARC_SLUG_CHAR_CLASSES); anything else is no ref of the box's
# (422 arc_ref_invalid). An arc is the named upstream commit cherry-picked onto the mirror's `dev` tip; it is never merged into `dev`.
# The slug grammar is data (the classes), never a regex in a rule. The conductor's selector, marker and basis rows are slice 2.
ARC_REF_PREFIX        = 'dev-'                  # an arc's ref on the SUT mirror = ARC_REF_PREFIX + slug
ARC_SLUG_CHAR_CLASSES = ('a-z', '0-9', '-')     # the slug grammar as data: lowercase letters, digits, hyphen
ARC_SLUG_MIN_LENGTH   = 1
ARC_SLUG_MAX_LENGTH   = 40
FORK_REFS             = ('dev', 'arc')          # the typed parse of a mirror ref the box accepts: Dev (the one internal line) | Arc(slug)
TRACK_KINDS = ('branch', 'tag')          # 0.2.179 §2 (G4): runs.track_kind, the Track enum's two variants (all 17 tracked runs are branch/main, measured)
ACTION_PLANES = ('customer', 'admin')   # 0.2.191 E73: actions.plane: the plane of the route that wrote the act; typed at write time, never a read-time role. A route writing an act sets its own plane; the Slack path writes customer from a bound customer channel, admin from an internal one. An Aretta act (0.2.180 §2, new_comments) is plane admin OR a non-person actor (service:*, the seat); new_comments resets on an Aretta act and counts the rest. History rows read customer by the DEFAULT (wrong only for staff comments before the column: state.md §12)
RUN_PURPOSES = ('customer', 'battery')   # 0.2.189 E65: runs.purpose; set by the conductor's dispatch (battery on track.upstream_moved), never a wire field (VerifyCreateReq unchanged); the stamp path appends run.battery_completed exactly when purpose = battery, never by matching a commit
ACCESSOR_STATES = ('present', 'absent')
# 0.2.177 §3 (the operator, actions.md §9 item 9): the campaign states are spec vocabulary; campaigns.db campaigns.status
# names it (the CHECK is probed equal); every state list is named <FACET>_STATES.
# 0.2.220 §1 (E122.2/5/30): being_prepared, staged and discarded leave; completed joins (E122.4). The two stored retired words stay
# in the CHECK for one release (§10.1: campaigns m27 widens; the narrowing waits on a measured zero); CAMPAIGN_STORED_STATES is
# the CHECK's set.
CAMPAIGN_STATES = ('launchable', 'launching', 'launched', 'completed', 'withdrawn', 'retired')
CAMPAIGN_STATES_RETIRED = ('being_prepared', 'discarded')
CAMPAIGN_STORED_STATES = CAMPAIGN_STATES + CAMPAIGN_STATES_RETIRED
CAMPAIGN_OUTCOMES = ('passed', 'failed', 'killed')   # campaign.completed's outcome (§10.4): done -> passed, failed -> failed, cancelled | parked -> killed
CAMPAIGN_TRANSITIONS = (                                                                               # (from, action, to)
    ('—',          'campaigns.card.author',    'launchable'),   # E122.30: author = create + finalize in one tx (§10.10); the same request causes strand::propose
    ('launchable', 'campaigns.card.feedback',  'launchable'),   # E122.1 (was decline), E122.31: also caused by strands.strand.comment on the card's strand when the presser holds curate_campaign
    ('launchable', 'campaigns.card.withdraw',  'withdrawn'),
    ('withdrawn',  'campaigns.card.repropose', 'launchable'),   # E122.29; the same request causes strand::propose (a new strand)
    ('launchable', 'campaigns.card.launch',    'launching'),    # E122.3: ONE synchronous forge request; also caused by strands.strand.accept on the card's strand (E122.51)
    ('launching',  'the forge created the issue',        'launched'),    # same request, TX2; writes campaign.launched
    ('launching',  'the forge refused',                  'launchable'),  # same request; the refund (credit.moved), E121/E122.3
    ('launching',  'reconcile: the issue exists',        'launched'),    # a stranded row older than CAMPAIGN_RECONCILE_AFTER_SECONDS (a clock event, R3)
    ('launching',  'reconcile: no issue',                'launchable'),  # the refund; writes campaign.launch_failed
    ('launched',   'the job finished',                   'completed'),   # E122.4: campaign.completed {campaign_id, job_id, outcome, has_output}, when the box's job settles
    ('launchable', 'campaigns.campaigns.retire', 'retired'),     # E122.6/38: org-wide offboarding; bookkeeping, stops nothing; per-card = withdraw
    ('launching',  'campaigns.campaigns.retire', 'retired'),
    ('launched',   'campaigns.campaigns.retire', 'retired'),
    ('withdrawn',  'campaigns.campaigns.retire', 'retired'),   # 0.2.221 §5 (E122.6 ext): withdrawn cards too; completed stays untouched
)
CAMPAIGN_SYSTEM_ACTS = ('the forge created the issue', 'the forge refused', 'reconcile: the issue exists', 'reconcile: no issue', 'the job finished')
CAMPAIGN_RECONCILE_AFTER_SECONDS = 300
CAMPAIGN_RECONCILE_EVERY_SECONDS = 120
# 0.2.190 §1 (E68.1): campaigns.source carries a CHECK; its words are this closed list (campaigns.db migration 25 widened it
# to `maintenance`). MAINTENANCE_CARD_SOURCE is the word the pipeline clock's in-process author writes.
CARD_SOURCES = ('fde_suggested', 'proposed_from_client_text', 'extracted_from_chat_gap', 'maintenance')   # campaigns.source; the CHECK's words
MAINTENANCE_CARD_SOURCE = 'maintenance'                                                                   # written by the clock's in-process author
# 0.2.181 E37 item 4: the customer wire's campaign status is a projection of CAMPAIGN_STATES, as data: {state: the wire word, or
# None = the row is omitted from the customer list (campaigns.cards.list, the only customer card read; there is no single-card
# read on the customer plane). MEASURED by c177 on the real binaries (GET /turso/api/client/v1/campaigns, both 200; evidence
# lanes/c177/measure/measure-output.txt sha256 1d4134e8...): the projection is crates/routes/src/client.rs to_wire :856-883,
# launching|launched -> derived_status (:879), so a launched card may read running, delivered or complete, derived from the
# forge observation and never stored. `discarded` leaves the codomain: the boot rules store a discarded row as withdrawn
# (or retired when charged), which is omitted.
# 0.2.220 §1, §11.11 (E122.7): the wire shows the STORED word: the identity on CAMPAIGN_STATES (DerivedStatus and the per-read
# derivation go) PLUS two explicit omissions for one release, being_prepared and staged (the 0.2.197 shape; an unknown stored word
# is otherwise read as Unvouchable). They leave the map when the drafts are gone (the measured zero).
CAMPAIGN_WIRE_STATUS = dict([(w, w) for w in CAMPAIGN_STATES] + [('being_prepared', None), ('staged', None)])
# 0.2.178 §5 (E37 release 1; c177's E31 point): `discarded` retires by a BOOT RULE on the campaigns write truth, not a migration.
# In release 1 the GLOBAL campaigns.db is the single write truth and the per-tenant files are derived copies at migration 24, so
# NO store is stamped 25. The head runs these, in this order, at every boot before serving, on the write truth (idempotent; no
# schema change; no version bump; like findings' boot_rule); each is total under the campaigns table's own CHECKs: (1) a CHARGED
# discarded row -> retired (keeps its spend; its status_reason meets retired_needs_a_reason); (2) a never-finalized one ->
# being_prepared with declined_at/by from status_set_at/by -- DEFENSIVE: human_owned_absent_iff_being_prepared makes such a row
# impossible to insert; (3) every other (finalized, uncharged) -> withdrawn, replaced_by NULL. CAMPAIGN_STATES and the status
# CHECK are unchanged this release; release 2 (after E31's release 2) runs these first in the per-tenant table rebuild, drops the
# word, and this rule retires.
CAMPAIGNS_BOOT_RULES = (
    "UPDATE campaigns SET status = 'retired' WHERE status = 'discarded' AND deducted_at IS NOT NULL",
    "UPDATE campaigns SET declined_at = COALESCE(declined_at, status_set_at), declined_by = COALESCE(declined_by, status_set_by) WHERE status = 'discarded' AND NOT (author IS NOT NULL AND estimated_by IS NOT NULL AND published_at IS NOT NULL AND finalized_by IS NOT NULL)",
    "UPDATE campaigns SET status = CASE WHEN author IS NOT NULL AND estimated_by IS NOT NULL AND published_at IS NOT NULL AND finalized_by IS NOT NULL THEN 'withdrawn' ELSE 'being_prepared' END WHERE status = 'discarded'",
)
OBSERVATION_SOURCES = ('pr_merge', 'run_verified')
# 0.2.177 §4 (the operator, actions.md §9 item 10's consolidation; item 9's naming): a finding has ONE state, replacing the
# two-word lifecycle (FINDING_LIFECYCLES, renamed) and the disclosed flag; findings.db migration 4's `state` names it (the two old
# columns stay in place, unwritten and unread, as `retired` columns: A4, their drop is E29). The
# issue link, the heal detail and the repair stay as fields (a repair is pending or not, orthogonal to the state).
FINDING_STATES = ('curated', 'disclosed', 'filed', 'healed', 'retracted')
# 0.2.221 §2 (E122.58, E122.60, E122.61, E122.63): a finding STORES two columns. visibility follows campaign: curate makes it
# DISCLOSABLE (the customer sees it; the same event opens the customer's strand), disclose publishes upstream at once
# (issue_number a column); `curated` → `disclosable` (the action and event stay curate / finding.curated), `filed` retires.
# liveness is ONE column (validity folds in): live | under_review | healed | retracted; `open` → `live`, `sound` retires.
# The old words stay in the store's CHECK for one release (read through the READ_MAPs, never written).
FINDING_VISIBILITY = ('disclosable', 'disclosed')
FINDING_VISIBILITY_RETIRED = ('curated', 'filed')
FINDING_VISIBILITY_STORED = FINDING_VISIBILITY + FINDING_VISIBILITY_RETIRED
FINDING_VISIBILITY_READ_MAP = {'curated': 'disclosable', 'filed': 'disclosed'}
FINDING_LIVENESS = ('live', 'under_review', 'healed', 'retracted')
FINDING_LIVENESS_RETIRED = ('open',)
FINDING_LIVENESS_STORED = FINDING_LIVENESS + FINDING_LIVENESS_RETIRED
FINDING_LIVENESS_READ_MAP = {'open': 'live'}
FINDING_FACETS = (('visibility', 'FINDING_VISIBILITY'), ('liveness', 'FINDING_LIVENESS'))
FINDING_TRANSITIONS = (   # (column, from, action, cause, to): `—` = no word yet; a cause names its event (a route id, `box.job.finished · <BOX_JOB_FINISHED_WORDS word>`, `intake.forge.receive · <forge word>`); `|` joins alternatives
    ('visibility', '—',           'curate',   'findings.finding.curate | intake.forge.receive · label.publish', 'disclosable'),   # the SAME event causes strand::propose (topic finding); writes the disclosure gate + approval columns
    ('visibility', 'disclosable', 'feedback', 'strands.strand.comment',                   'disclosable'),   # E122.61: mirrored onto the forge staging issue; writes finding.feedback
    ('visibility', 'disclosable', 'revise',   'intake.forge.receive · issue.edited',      'disclosable'),   # a staff edit of the staging issue: a new version on the strand
    ('visibility', 'disclosable', 'disclose', 'strands.strand.accept | findings.finding.disclose', 'disclosed'),   # publishes UPSTREAM AT ONCE: the issue is opened, issue_number a column
    ('visibility', 'disclosed',   'revise',   'intake.forge.receive · issue.edited',      'disclosed'),     # E122.63: replicated as a comment upstream (deferred under a hold); the aretta-lead block stays stripped
    ('liveness',   '—',           'curate',   'findings.finding.curate | intake.forge.receive · label.publish', 'live'),   # the same curate
    ('liveness',   'live',        'heal',     'intake.webhook.receive · pull_request.merged | findings.heal.redrive', 'live'),   # requests nothing: the same event causes run::start (purpose heal); refused unless live (409 not_live)
    ('liveness',   'live',        'healed',   'box.job.finished · heal_reverify.green',   'healed'),
    ('liveness',   'live',        'triage',   'box.job.finished · heal_reverify.red',     'live'),          # E122.60: requests box.jobs.submit {purpose: triage}; writes triage.requested
    ('liveness',   'live',        'incomplete','box.job.finished · heal_reverify.incomplete', 'live'),
    ('liveness',   'live',        'stands',   'box.job.finished · triage.bug_persists | findings.triage.verdict | intake.forge.receive · label.verdict', 'live'),   # 0.2.224 §3: was `persist` ("persist" reads as "make persistent"); bug_persists | uncertain: reverify_failed; the upstream issue reopened (deferred under a hold)
    ('liveness',   'live',        'review',   'findings.triage.verdict | intake.forge.receive · label.verdict', 'under_review'),   # over_strict_dt: proof_repair_pending; 0.2.224 §5: a PERSON's verdict only, never a box word (a machine never puts Aretta's own claim under review)
    ('liveness',   'live',        'escalate', 'box.job.finished · triage.uncertain',      'live'),          # E122.60: the staff triage strand (topic triage) + the triage issue on the forge; writes triage.filed
    ('liveness',   'under_review','insist',   'findings.repair.reject',                   'live'),          # the customer: the problem is real; admitted only here (409 not_under_review)
    ('liveness',   'live',        'retract',  'findings.finding.retract',                 'retracted'),
    ('liveness',   'under_review','retract',  'findings.finding.retract',                 'retracted'),     # a finished finding is not retracted (409 finished)
)
FINDING_TRANSITION_VERDICTS = (('stands', ('bug_persists', 'uncertain')), ('review', ('over_strict_dt',)))   # which TRIAGE_VERDICTS word a verdict cause names, per action (0.2.224 §3: stands, was persist)
# 0.2.220 §3b (the heal): its triggers, preconditions, plan and outcomes, as data (actions.md §3.3b)
HEAL_TRIGGERS = ('intake.webhook.receive: a merged PR whose body closes a finding issue', 'findings.heal.redrive', 'the reconcile sweep meets a merged PR the webhook missed')
HEAL_PRECONDITIONS = ('already_healed', 'judged_for_rev', 'heal_switch_off', 'clock_paused')       # the route form refuses 409 on the first two; the webhook form skips
HEAL_ROUTE_REFUSES = ('already_healed', 'judged_for_rev')                                          # ⊂ HEAL_PRECONDITIONS: 409 on findings.heal.redrive
HEAL_PLANS = (   # the FINAL's HEAL_PLAN, named plural (0.2.183 Rule A)
             'battery: the served edition has runnable coverage for the finding\'s spec anchors → a grounding verify as service:conductor at the fix PR\'s merge sha',
             'witness: else the narrative\'s repro_cmd in the sandbox at the same sha')
HEAL_OUTCOMES = ('green', 'red', 'incomplete')                                                       # red files the triage issue; a person labels ONE verdict
HEAL_OUTCOME_RECORDS = (('green', ('finding.healed',)), ('red', ('triage.requested',)), ('incomplete', ('finding.reverify_incomplete',)))   # what each outcome writes; 0.2.221 §2: red requests the triage JOB (triage.requested) and writes NO finding.* event
TRIAGE_VERDICTS = ('over_strict_dt', 'bug_persists', 'uncertain')                                    # 0.2.221 (E122.59): route words of findings.triage.verdict (a PERSON's words); the label words verdict:over-strict-dt | bug-persists | uncertain; "yellow" is never a word; 0.2.224 §5: the box's words are BOX_TRIAGE_WORDS
# 0.2.224 §5 (RULED, the operator 2026-11-04): the words the box's triage job settles with (`box.job.finished · triage.<word>`), a SUBSET of
# TRIAGE_VERDICTS: a machine never puts Aretta's own claim under review, so `over_strict_dt` stays a PERSON's verdict only (the route
# word, the forge label). The conductor's handling of over_strict is unchanged (one cause fewer); the box's triage recipe (not yet built)
# emits these two words. E124 (51) O1: the triage job settles with a STRUCTURED outcome: the box's job-finished callback carries outcome = the
# job's result field `verdict` (one of these words, written by the job as data); the box reports the finished word triage.<verdict>, never
# summary text. Block 13 builds the conductor's handling of the two words; the dispatch turns on only when the box half (the triage recipe,
# the next box cut) is live; until then the forge triage path stands. validate: exactly the triage.* BOX_JOB_FINISHED_WORDS, named by the
# `stands` action, never by `review`.
BOX_TRIAGE_WORDS = ('bug_persists', 'uncertain')
TRIAGE_JOB_SPEC_FIELDS = (('kind', 'agent'), ('runner', 'claude'), ('purpose', 'triage'), ('inputs', ('finding', 'the check', 'the fix sha', 'the red run evidence')))   # 0.2.221 §2: settles with exactly one TRIAGE_VERDICTS word
TRIAGE_VERDICT_LABEL_MAP = "'_' -> '-', prefixed verdict:"                                          # TRIAGE_VERDICTS word -> its labels_vocab LABELS name
FINDING_ACTS = ('curated', 'revised', 'issue_linked', 'disclosed', 'healed', 'feedback')   # 0.2.221 E124 (35) (cAuthP1): feedback joins (findings m8 rebuilds finding_history's CHECK)
# 0.2.220 §5 (E120): a finding's `upstream` word, ONE closed set shared by books (the amend verb writes it; the kit validates it)
# and the conductor (ships it verbatim on the customer row); a ledger-only (pre-cut) finding reads the default `none`, never a
# word outside the set; the lenient readers' synonyms (merged, resolved, patched, declined, wont-fix, rejected, open) go.
FINDING_UPSTREAM_STATES = ('none', 'reported', 'fixed', 'wontfix')
FINDING_UPSTREAM_DEFAULT = 'none'
FINDING_ORIGINS = ('curated', 'edition')     # 0.2.179 §2 (G2): how a finding row came to exist: curated on this chain, or served in an edition and never curated
DISCLOSURE_CHANNELS = ('dashboard', 'report', 'api')
BUG_SOURCES = ('verify', 'discovery')
BUG_DISPOSITIONS = ('confirmed', 'wontfix', 'retracted')
# 0.2.220 §4 (E122.43): the disposition is the state; fixed_pending_verify and fixed leave (no writer since 2026-07-17), stay in the
# store's CHECK (bugs.db unchanged) and read as open (§8.4)
BUG_STATES = ('open', 'confirmed', 'wontfix', 'retracted')
BUG_STATES_RETIRED = ('fixed_pending_verify', 'fixed')
BUG_STORED_STATES = BUG_STATES + BUG_STATES_RETIRED
BUG_STATE_READ_MAP = {'fixed_pending_verify': 'open', 'fixed': 'open'}
BUG_VERDICTS = ('satisfied', 'untested', 'violated')
# 0.2.220 §4 (E122.43): the DISPOSITION is the state; `open` = no disposition (the badge's word). (from, by, to): the opening
# arrives over verify.events.submit from a service:bug-ledger credential; each dispose writes bug.verdict with the disposition
# and the stored word (E122.16, R1). BUG_SOURCES `verify` is unbuilt in practice (E122.44).
BUG_TRANSITIONS = (('—', 'the in-process bug-ledger opens the bug', 'open'),   # 0.2.220 §10.5: no out-of-process submitter; BUG_SOURCES verify is unbuilt until then
                   ('*', 'findings.bug.dispose confirmed', 'confirmed'), ('*', 'findings.bug.dispose wontfix', 'wontfix'), ('*', 'findings.bug.dispose retracted', 'retracted'))
EDITION_SLOTS = ('served', 'rollback')
# 0.2.221 §1 (E122.52, E122.64): an edition STORES its word (R1); `regressed` is NOT stored (read from the run). Each row is (from,
# action, cause, to); a cause names its event; `(the same event)` marks the R4 shape (one event, two facets' actions).
EDITION_STATES = ('proposed', 'cut', 'served', 'superseded', 'skipped')
EDITION_TRANSITIONS = (
    ('—',          'propose',  'box.job.finished · cut_job.done',                     'proposed'),    # the box's staging pass opened the cut PR as the tenant bot; id by tools/atlas-cut.sh
    ('proposed',   'cut',      'box.job.finished · cut_pr_check.green',               'cut'),         # AUTOMATIC: head-bound merge as the bot; writes edition.cut
    ('proposed',   'cut',      'intake.forge.receive · pull_request.merged',          'cut'),         # the hand merge stays
    ('proposed',   'drop',     'box.job.finished · cut_pr_check.red',                 'skipped'),     # edition.dropped {reason: check_red}
    ('proposed',   'drop',     'intake.forge.receive · pull_request.closed',          'skipped'),     # edition.dropped {reason: pr_closed}
    ('cut',        'cut',      'a newer cut landed (the same event)',                 'skipped'),     # the newest cut is the candidate; older unserved cuts skip
    ('cut',        'verify',   'edition.cut (the same event)',                        'cut'),         # the candidate verify: the cut event also causes run::start (R4 amended)
    ('cut',        'judge',    'box.job.finished · candidate_verify.pass | box.job.finished · candidate_verify.fail | box.job.finished · candidate_verify.inconclusive', 'cut'),   # writes candidate_verdict
    ('cut',        'promote',  'box.job.finished · candidate_verify.pass',            'served'),      # when NOT customer_visible_change: automatic promote (E122.64: no seat press)
    ('cut',        'promote',  'strands.strand.accept | books.edition.promote',       'served'),      # customer_visible_change: the strand; a staff press anytime
    ('served',     'promote',  'another edition promoted (the same event)',           'superseded'),
    ('superseded', 'promote',  'books.edition.promote',                               'served'),      # the ROLLBACK = a promote back; ROLLBACK_GUARDS read it at press time; no flag, no reason field
    ('served',     'measure',  'edition.advanced (the same event)',                   'served'),      # the external verify <= 24 h: also causes run::start
    ('served',     'alarm',    'box.job.finished · external_verify.done',             'served'),      # a notice when regressed; `regressed` is read from the run, never stored
    ('served',     'remeasure','track.reset (the same event)',                        'served'),      # 0.2.212 served_remeasure (basis_only): also causes run::start
    ('served',     'unmeasured','clock.edition.unmeasured',                           'served'),      # the one true clock of edition: 24 h since edition.advanced; 0.2.224 §3: was `nag` (tone)
)
EDITION_CANDIDATE_VERDICTS = ('none', 'pass', 'fail', 'inconclusive')
EDITION_COLUMNS = (('candidate_verdict', 'EDITION_CANDIDATE_VERDICTS'), ('candidate_run_seq', 'integer'), ('candidate_judged_at', 'time'))   # written by judge
CUT_JOB_SPEC_FIELDS = (('kind', 'agent'), ('runner', 'claude'), ('repos', ('books fork @ dev', 'primary')), ('task_branch', 'cut/*'), ('pull_limit', 0),
                ('identity', 'aretta-conductor-bot'), ('token', 'the conductor box operator token'))   # E122.52 §E4(a), as the code runs it
# 0.2.221 §6 R6 (E122.59): a route whose body picked the action carries the words in its PATH: (route id, path parameter, the
# vocabulary tuple of its words); a word outside the tuple is an unknown route, never 400 invalid_body
STRAND_ACCEPT_ONLY_ROWS = ('mirrors.track.reset', 'books.edition.promote')   # 0.2.221 §1 (E124 (15)): admin POSTs at read reached by readers ONLY through strands.strand.accept (the admin HTTP plane stays GET-only for persons)
PATH_WORDS = (('findings.triage.verdict', 'verdict', 'TRIAGE_VERDICTS'),)
# 0.2.221 (E122.50): the route events the transitions name whose rows land with the forge webhook (a third peer holder `forge`,
# credential kind forge_webhook_signature; cut C / E122.50): named, not yet registry rows
PROPOSED_ROUTE_EVENTS = ()   # 0.2.235 §1: intake.forge.receive's row landed (INTAKE_FORGE_ROUTE); nothing is proposed
CUT_PRECONDITIONS = ('one open cut PR per repo', 'the PR head is the sha the checks were seen on (409 head_moved)', 'both ci and scan statuses are success')
CUSTOMER_VISIBLE_CHANGE = 'released spec ids(new) != released spec ids(served) or disclosed finding ids(new) != disclosed finding ids(served)'   # membership only; content changes do not count
CUT_DISPATCH_SKIP_REASONS = ('no_seat', 'no_image', 'no_verify_dispatch')   # 0.2.221 E124 (25) O6: notify.cut_dispatch_skipped's reason
CUT_KNOB_DEFAULTS = (('cut_dispatch', True), ('cut_auto_merge', True))   # 0.2.221 §1: default ON (today absent = off): new behaviour for every org
EDITION_CLOCKS = (('clock.edition.unmeasured', 24 * 3600),)   # (clock event, seconds since edition.advanced)
# 0.2.221 §3 (E122.62): a run (a verify session) STORES its word; `queued` retires (never written; read on the wire for one
# release); `lost` is failed {reason: lost}, by the one true clock clock.run.lost.
RUN_STATES = ('requested', 'running', 'waiting_image', 'done', 'failed', 'cancelled')
RUN_STATES_RETIRED = ('queued', 'lost')
RUN_STORED_STATES = RUN_STATES + RUN_STATES_RETIRED
RUN_STATE_READ_MAP = {'queued': 'requested', 'lost': 'failed'}
RUN_FAIL_REASONS = ('lost',)
RUN_TRANSITIONS = (   # (from, action, cause, to)
    ('—',             'start',    'verify.sessions.start | intake.webhook.receive · pull_request.merged | findings.heal.redrive | edition.cut (the same event) | edition.advanced (the same event) | track.reset (the same event) | track.upstream_moved (the same event)', 'requested'),   # requests box.jobs.submit; writes run.requested {step}; 0.2.224 §4: a start with purpose battery is NOT started while a battery is in flight at that commit (BATTERY_RULES)
    ('—',             'hold',     'the same open, when the verify image track_ref differs from the pinned image', 'waiting_image'),   # writes run.held
    ('waiting_image', 'release',  'live.image.report | box.job.finished · image_bake.built', 'requested'),   # requests box.jobs.submit
    ('waiting_image', 'cancel',   'verify.sessions.cancel',                   'cancelled'),
    ('requested',     'dispatch', 'box.job.started',                          'running'),      # the box took the job; writes run.started (the watchdog anchor)
    ('requested',     'cancel',   'verify.sessions.cancel',                   'cancelled'),
    ('running',       'stamp',    'box.job.finished · verify.done | box.job.finished · battery.done | box.job.finished · candidate_verify.pass | box.job.finished · candidate_verify.fail | box.job.finished · external_verify.done | box.job.finished · heal_reverify.green | box.job.finished · heal_reverify.red', 'done'),   # the conductor's in-process stamp; the same event drives edition::judge, track::settle, finding::healed / triage
    ('running',       'fail',     'box.job.finished · verify.failed | box.job.finished · battery.failed | box.job.finished · candidate_verify.inconclusive | box.job.finished · external_verify.failed | box.job.finished · heal_reverify.incomplete', 'failed'),
    ('running',       'lost',     'clock.run.lost',                           'failed'),       # verify.lost_after_seconds since run.started; today the watchdog sweep; 0.2.224 §3: was `lose` (the verb)
    ('running',       'cancel',   'verify.sessions.cancel',                   'cancelled'),
)
RUN_COLUMNS = (('state', 'RUN_STORED_STATES'), ('step', 'VERIFY_STEP_WORDS'), ('requested_by', 'the actor word (the cause\'s holder)'))   # E124 (26): the session JSON's fields (runs/<repo>/sessions/), NOT runs.db (unchanged this cut)   # step replaces the marker file (0.2.215), read by the deploy hold
# 0.2.221 §1–§3, §6: the box's job-finished words the transitions name (`box.job.finished · <word>`), as data; box.job.started is
# the run's dispatch event. A job settles with exactly one word.
BOX_JOB_FINISHED_WORDS = ('verify.done', 'verify.failed', 'battery.done', 'battery.failed',
                          'candidate_verify.pass', 'candidate_verify.fail', 'candidate_verify.inconclusive',
                          'external_verify.done', 'external_verify.failed',
                          'heal_reverify.green', 'heal_reverify.red', 'heal_reverify.incomplete',
                          'cut_job.done', 'cut_pr_check.green', 'cut_pr_check.red', 'image_bake.built',
                          'triage.bug_persists', 'triage.uncertain',   # 0.2.224 §5: triage.over_strict_dt REMOVED from the box demux (BOX_TRIAGE_WORDS)
                          'image_bake.failed')   # 0.2.222 §2: a bake that fails (the job ends without a built report, 0.2.202 §3b); image_bake.* exist as the FINAL says, the failed word named here
BOX_JOB_STARTED = 'box.job.started'
FORGE_EVENT_WORDS = ('pull_request.merged', 'pull_request.closed', 'issue.edited', 'label.publish', 'label.verdict',   # intake.forge.receive's words the transitions name (E122.50)
                     'push', 'installation.created', 'installation.suspended', 'installation.unsuspended', 'installation.deleted',   # 0.2.222 §1, §3: intake.webhook.receive's words (the GitHub App's push and installation events)
                     'member.removed')   # 0.2.237 §1: the App webhook's arm the credential facet's purge names (registry RT__INTAKE_WEBHOOK_RECEIVE.on("member.removed"); hooks.rs:2445 at 6e15f706)
# 0.2.222 §0 R7 (E122.66): the spec models CONTROL state only. A word exists only where a decision branches (R2); everything else
# the system holds is DATA state (settings, balances, profiles, admin writes, syncs), documented as a ROUTE WITH AN ACTION (door,
# request, postcondition, witness or `records: []`), never as a transition. The *_MECHANISM, INSTALLATION_SETTINGS and
# CREDIT_MOVES tuples below are R7's data: route ids (validate: live rows), no transitions.
# 0.2.222 §1 (E122.67): a track STORES its word (editions.db tracks, m3); the POSITION is a column pair. Today's config words
# accepted | upstream_moved read as tracking (the move is an event), kept in the CHECK one release (TRACK_STATE_READ_MAP).
TRACK_STATES = ('requested', 'tracking', 'locked')
TRACK_STATES_RETIRED = ('accepted', 'upstream_moved')
TRACK_STORED_STATES = TRACK_STATES + TRACK_STATES_RETIRED
TRACK_STATE_READ_MAP = {'accepted': 'tracking', 'upstream_moved': 'tracking'}
TRACK_FLOOR_STATES = ('requested', 'accepted', 'locked', 'upstream_moved')   # today's words (the config ledger's track.* lines + advance_locked): what the floor reads; the floor_boot_rule lands every row on one (validate probes it); for images the floor's words are IMAGE_REQUEST_STATES
TRACK_TRANSITIONS = (   # (from, action, cause, to); a cause names its event; the precondition that splits one box word is TRACK_BATTERY_OUTCOMES
    ('—',         'request',      'setup.repo.put | setup.repo.put@repo',                        'requested'),   # a config commit; the same event causes run::start (purpose battery, at the requested commit) and strand::propose (topic track, the customer's)
    ('tracking',  'request',      'setup.repo.put | setup.repo.put@repo',                        'requested'),   # a new track: the old position serves until accepted; refused while locked (409 locked)
    ('requested', 'accept',       'box.job.finished · battery.done',                             'tracking'),    # AUTOMATIC: no failing units, at the REQUESTED commit; the position is written; the track strand closes accepted with no press (strand::expire, track_accepted)
    ('requested', 'not_ready',    'box.job.finished · battery.done | box.job.finished · battery.failed', 'requested'),   # failing units, or battery.failed, at the requested commit; the same settle authors the maintenance card (campaign::author); 0.2.224 §3: was `hold` (`hold` also names run's image wait)
    ('requested', 'move',         'intake.webhook.receive · push',                               'requested'),   # E124 (36) O1: RECORDED (track.upstream_moved, a self-loop): no battery, the old position does not move, the request stands
    ('tracking',  'lock',         'live.track_state.lock',                                       'locked'),      # staff; a dashboard button (CALLERS_SET)
    ('locked',    'unlock',       'live.track_state.unlock',                                     'tracking'),    # E124 (36) O2: nothing fires by itself afterwards; the next push or request_sync runs the battery
    ('tracking',  'move',         'intake.webhook.receive · push',                               'tracking'),    # a push on the tracked default branch: writes track.upstream_moved; the same event causes run::start (battery at the new commit) unless advance = manual (O3)
    ('locked',    'move',         'intake.webhook.receive · push',                               'locked'),      # recorded; the battery runs; a move needs an unlock first
    ('tracking',  'request_sync', 'mirrors.track.request_sync',                                  'tracking'),    # the customer: writes track.sync_requested; causes run::start (battery at upstream head); 503 maintenance_unconfigured when the price is unset
    ('tracking',  'settle',       'box.job.finished · battery.done | box.job.finished · battery.failed', 'tracking'),   # at a MOVED commit (0.2.224 §4: the settle reads the track's own row, BATTERY_RULES): run.battery_completed {repo, run_seq, to_commit, failing_units}; 0 -> strand::propose (track_move), else campaign::author (the maintenance card)
    ('tracking',  'reset',        'strands.strand.accept | mirrors.track.reset',                 'tracking'),    # the position moves; writes track.reset (the cut and served_remeasure fire on it, 0.2.212); readers reach the reset only through the strand's accept (STRAND_ACCEPT_ONLY_ROWS)
)
TRACK_BATTERY_OUTCOMES = (   # one box word (battery.done) causes accept | not_ready | settle, disjoint by precondition (requested vs moved commit; green vs red); 0.2.224 §3: not_ready was hold
    ('accept',    'battery.done, no failing units, at the requested commit'),
    ('not_ready', 'battery.done with failing units | battery.failed, at the requested commit'),
    ('settle', 'battery.done | battery.failed at a moved commit'),
)
# 0.2.224 §4 (the operator 2026-11-04; the two-batteries corner case): ONE BATTERY PER COMMIT, as data. (step, the rule). start: run::start
# with purpose battery is NOT started when a battery is already in flight at that commit (the same precondition as the candidate verify,
# 409 verify_in_flight: the codes_vocab word the FINAL calls run_in_flight, accepted by vyasa); the second cause attaches to the running one;
# E124 (51) O4: a request made while a battery is in flight at C is RECORDED (track.requested) and attached, that battery's settle serves it
# (the track row decides), the refusal is for the second run start only, never the request; no tag on the run. settle: on a battery settle at commit C the track reads ITS OWN ROW: a request naming C -> accept (green) | not_ready (red);
# otherwise a move to C -> settle (green: the track_move strand; red: the maintenance card); both pending -> the accept wins (the
# position becomes C; the move is moot). validate: the start rule names a 409 code; the settle rule names every TRACK_BATTERY_OUTCOMES action.
BATTERY_RULES = (
    ('start',  'one battery per commit: not started while a battery is in flight at that commit (409 verify_in_flight: refuses only the second RUN start, never the request); the second cause attaches to the running one (a request made meanwhile is RECORDED, track.requested, and served by that battery\'s settle: the track row decides, E124 (51) O4); no tag on the run'),
    ('settle', 'the track reads its own row: a request naming C -> accept (green) | not_ready (red); else a move to C -> settle; both pending -> the accept wins (the position becomes C; the move is moot)'),
)
# 0.2.224 §3 (the operator 2026-11-04; words only, no transition changes): the four action renames, (facet, old, new). Ledger events,
# routes and wire words are unchanged (the action name is the registry's and the docs'). validate: the new word is an action of the
# facet's transitions and the old word is not; the run's `hold` (the image wait) stays, which is why the track's went. E124 (51) O3: no
# stored row carries an action name (ledger events, routes, states, history acts unchanged), so the renames need no read map.
ACTION_RENAMES = (
    ('track',   'hold',    'not_ready'),    # `hold` also names run's image wait
    ('finding', 'persist', 'stands'),       # "persist" reads as "make persistent"
    ('edition', 'nag',     'unmeasured'),   # tone
    ('run',     'lose',    'lost'),         # the verb
)
TRACK_ADVANCE_WORDS = ('auto', 'manual')   # org.toml repos[].advance: a SETTING read by the webhook (manual suppresses only the automatic battery on a push; never an accept or a request_sync, O3)
TRACK_POSITION_SOURCES = ('run', 'reset', 'accept')   # 0.2.226 E124 (83): editions.db tracks.position_from — what last moved the position: a run's stamp, an admin reset, an accept
TRACK_COLUMNS = (('state', 'TRACK_STORED_STATES'), ('position_commit', 'string'), ('position_line', 'string'), ('advance', 'TRACK_ADVANCE_WORDS'),   # editions.db tracks (m3)
                 ('requested_commit', 'string'),   # 0.2.225 §3b
                 ('position_from', 'TRACK_POSITION_SOURCES'), ('position_at', 'time'), ('position_seq', 'integer'), ('last_reset_seq', 'integer'), ('last_reset_at', 'time'))   # 0.2.226 E124 (83), m9 (additive): the position's provenance (what moved it, when, the record's seq) and the last reset's seq and time; NULL on a row the floor writes (E124 (60), m5): the SUT commit the open request resolves to (from track.requested's requested_commit), NULL when none is open; accept clears it; validate: exactly the tracks columns beside the key and ledger_seq
TRACK_MECHANISM = ('mirrors.track.sync', 'mirrors.clone.sync', 'mirrors.mirror.sync', 'mirrors.accessor_prs.sweep', 'mirrors.capabilities.fold')   # R7: routes with actions; records: [] (the sweep: instrumentation.observed); no transitions
# 0.2.222 §2 (E122.65, E122.68): an image STORES its word (editions.db images, m4: one row per kind); build_failed / probe_failed are
# NOT states (the failure sits on the bake line: last_bake_failure); today's dispatched reads as baking. IMAGE_REQUEST_STATES stays
# the WIRE's tuple (ImageView.state) for one release: build_failed / probe_failed retire as state words, read through the map.
IMAGE_STATES = ('requested', 'accepted', 'baking', 'built')
IMAGE_BAKE_FAILURES = ('build_failed', 'probe_failed')   # the bake line's failed state words (the config ledger's image.built state minus built); editions.db images.last_bake_failure
IMAGE_STATES_RETIRED = ('dispatched',) + IMAGE_BAKE_FAILURES
IMAGE_STORED_STATES = IMAGE_STATES + IMAGE_STATES_RETIRED
IMAGE_STATE_READ_MAP = {'dispatched': 'baking', 'build_failed': 'accepted', 'probe_failed': 'accepted'}
IMAGE_TRANSITIONS = (   # (from, action, cause, to)
    ('—',         'request',  'setup.image.put | setup.image.put@repo',                       'requested'),   # the same request causes strand::propose (topic image, audience INTERNAL: the accept button on the dashboard / Slack) — E122.68
    ('requested', 'accept',   'strands.strand.accept | live.image.accept',                    'accepted'),    # a HUMAN act (a bake is heavy): never automatic; the strand closes accepted
    ('accepted',  'dispatch', 'image.accepted (the same event)',                              'baking'),      # requests box.jobs.submit {image_bake}; precondition: no open dispatch for this commit
    ('accepted',  'rebake',   'live.image.accept',                                            'baking'),      # when the last bake line is a failure and no dispatch is open: Aretta fixes and re-bakes (today the hand command)
    ('baking',    'built',    'live.image.report | box.job.finished · image_bake.built',      'built'),       # the pin in org.toml; the same event causes run::release for held runs
    ('baking',    'fail',     'live.image.report | box.job.finished · image_bake.failed',     'accepted'),    # image.built carries {state: build_failed | probe_failed, reason}; the request stands; customer notice pending_reason_build_failed
    ('built',     'unpin',    'live.image.unpin',                                             'requested'),   # E124 (36) O4: ALSO causes strand::propose (topic image, internal): the request stands and needs a fresh accept
)
IMAGE_KINDS = ('verify', 'sandbox')   # the config ledger's image_kind (validate: equal); editions.db images.kind
# 0.2.226 §2 (E122.75): the edition facet's history as rows (editions.db m6, m7; additive). PROMOTIONS: one row per promote (the advances
# fold: picture.overview.get's rollback flag, live.org.get's history); BAKES: one row per image.built line (the pin history of live.image.get
# and the ImageCard's last bake). bakes.state is the config ledger's image.built `state` enum (IMAGE_BUILT_STATES) and bakes.reason its
# `reason` enum (IMAGE_BAKE_REASONS); `failure` is the report's failure word (free, NULL when built). validate: each table's columns
# beside the key and ledger_seq are exactly its tuple; the two enums equal the config-ledger schema's.
IMAGE_BUILT_STATES = ('built', 'build_failed', 'probe_failed', 'unpinned')   # editions.db bakes.state = configevents image.built state + `unpinned` (0.2.227 §1, E124 (91): an image.unpinned line is a bakes row too — {kind, at, state unpinned}, every other column NULL; the pin history = the kind's rows by ledger_seq (0.2.228): a built row pushes, an unpinned row pops)
IMAGE_BAKE_REASONS = ('bake', 'rollback', 'custom', 'seed')      # editions.db bakes.reason = configevents image.built reason
PROMOTION_COLUMNS = (('from_edition', 'string'), ('to_edition', 'string'), ('actor', 'string'))   # editions.db promotions (m6); key (edition_id, at)
BAKE_COLUMNS = (('request_commit', 'string'), ('state', 'IMAGE_BUILT_STATES'), ('failure', 'string'), ('reason', 'IMAGE_BAKE_REASONS'),   # editions.db bakes (m7); key (kind, ledger_seq) since m10 (0.2.228, E124 (96): every row comes from one config line; `at` is a plain column; the pin history orders by ledger_seq); 0.2.227 §1 (m10): reason NULL on an unpinned row
                ('digest', 'string'), ('recipe', 'string'), ('plan_digest', 'string'), ('probe_record', 'json'),   # 0.2.227 §1 (E124 (91), m10): the pin entry's fields, carried from the image.built line at the same write and by the bakes backfill; NULL on a failed bake and on an unpinned row
                ('track_ref', 'string'), ('artifact_version', 'string'), ('note', 'string'))   # 0.2.230 §3 (E124 (97), m11): the entry's last three fields (errata 0.1.1's tag-layer fields and the report's note), the same carry; NULL on a failed bake and an unpinned row; live.image.get's history entry serves them again
IMAGE_COLUMNS = (('state', 'IMAGE_STORED_STATES'), ('last_bake_failure', 'the last bake line\'s failed state word (build_failed | probe_failed), NULL once a bake builds'), ('last_bake_reason', 'the report\'s failure word'))
IMAGE_MECHANISM = ('setup.image.check', 'setup.image.check@repo')   # R7: routes with actions, no transitions; the boot re-poll of an open dispatch (0.2.202 §3b) is in-process, no row
# 0.2.222 §3 (E122.66): CONTROL state only; the GitHub App's installation events move it (org.toml installation_state, INSTALLATION_STATES above)
INSTALLATION_TRANSITIONS = (
    ('—',         'install', 'intake.webhook.receive · installation.created',     'active'),
    ('deleted',   'install', 'intake.webhook.receive · installation.created',     'active'),      # E124 (36) O7: a re-install
    ('active',    'suspend', 'intake.webhook.receive · installation.suspended',   'suspended'),
    ('suspended', 'resume',  'intake.webhook.receive · installation.unsuspended', 'active'),
    ('active',    'delete',  'intake.webhook.receive · installation.deleted',     'deleted'),
    ('suspended', 'delete',  'intake.webhook.receive · installation.deleted',     'deleted'),
)
INSTALLATION_SETTINGS = ('setup.config.complete', 'setup.contacts.put', 'live.logo.put', 'campaigns.label_map.put', 'setup.repo_features.put', 'live.platform.put')   # the FINAL's setup.label_map.put is the registry's campaigns.label_map.put (a reading)   # R7: DATA state, routes with actions witnessed by the config commit; onboarding.completed is a SETTING (no route is refused before it)
INSTALLATION_REMOVE = ('live.platform.put', 'repo.removed')   # §3: `remove` is its own action and event: the platform write that drops a repo from repos[] writes repo.removed as its witness (the write's other keys are a setting, records: [])
# 0.2.222 §4 (E122.66): the org's Slack binding, org.toml [notify.slack]; the bind is AUTOMATIC (the staff bind retires); the clear stays
NOTIFY_STATES = ('unbound', 'requested', 'bound')
NOTIFY_TRANSITIONS = (
    ('—',         'configure', 'notify.slack.put',                                              'requested'),   # the customer picks channel + events, no Slack install yet
    ('unbound',   'configure', 'notify.slack.put',                                              'requested'),
    ('requested', 'bind',      'notify.install.receive',                                        'bound'),       # AUTOMATIC: Slack's OAuth callback when a channel is picked
    ('—',         'bind',      'notify.slack.put',                                              'bound'),       # AUTOMATIC: when the install exists
    ('unbound',   'bind',      'notify.slack.put',                                              'bound'),
    ('bound',     'bind',      'notify.slack.put',                                              'bound'),       # a re-pick: stays bound; never through unbound
    ('bound',     'unbind',    'notify.binding.put | notify.events.receive · app_uninstalled | notify.events.receive · tokens_revoked', 'unbound'),   # the staff clear; E124 (36) O8: Slack's app_uninstalled / tokens_revoked words
)
NOTIFY_BINDING_WORDS = ('clear',)   # §4: notify.binding.put loses `bind` (the staff bind retires): the body's enabled is false; interactive_enabled / disabled is a column
NOTIFY_SLACK_EVENT_WORDS = ('app_uninstalled', 'tokens_revoked')   # notify.events.receive's words the transitions name (Slack's Events API)
NOTIFY_MECHANISM = ('notify.binding.check', 'notify.internal.check', 'notify.interactions.receive', 'notify.slack.check')   # 0.2.226 §6a (E124 (71)): the customer's own check through the org's install (answers {ok, is_member, channel})   # R7: test posts; the button-press TRANSPORT (R3: a route event of whatever the button presses)
# 0.2.222 §5 (R7): credit is a balance, no machine: every move writes credit.moved (validate: each route's records carry it)
CREDIT_MOVES = (('set', 'campaigns.org_credits.put'), ('draw', 'campaigns.card.launch'), ('draw', 'strands.strand.accept'),
                ('refund', 'clock.campaign.stranded_launch'), ('chat_draw', 'conversations.chat.create'))
# 0.2.179 §3 (G3, G5, G6): the fold's words -> the stores' vocabularies; every map is TOTAL, and validate holds the two sets
# equal after the map. The maps are the draft's (CAPABILITY_WORD_MAP 'lowercase', BUG_STATE_WORD_MAP "'-' -> '_'"); the fold's
# words they map from are data tuples here (G4, ruled): the capability words measured by cBackfill on the prod copy (wire
# PRESENT / RUN_VERIFIED); the bug words the conductor's LifecycleState serde (kebab), code-read, 0 rows on the copy (UNMEASURED).
CAPABILITY_WORD_MAP = 'lowercase'           # wire PRESENT / RUN_VERIFIED -> ACCESSOR_STATES / OBSERVATION_SOURCES
BUG_STATE_WORD_MAP  = "'-' -> '_'"          # the fold's kebab (fixed-pending-verify) -> BUG_STATES (fixed_pending_verify)
FOLD_ACCESSOR_WORDS  = ('PRESENT', 'ABSENT')          # measured (cBackfill, the prod copy)
FOLD_SOURCE_WORDS    = ('PR_MERGE', 'RUN_VERIFIED')     # measured (cBackfill, the prod copy)
FOLD_BUG_STATE_WORDS = ('open', 'confirmed', 'fixed-pending-verify', 'fixed', 'wontfix', 'retracted')   # UNMEASURED: code-read serde, 0 rows on the copy
FOLD_WORDS = (   # (store vocabulary, the map, the fold's words): each map is validated as a function from its tuple onto the vocabulary
    ('ACCESSOR_STATES',     'CAPABILITY_WORD_MAP', FOLD_ACCESSOR_WORDS),
    ('OBSERVATION_SOURCES', 'CAPABILITY_WORD_MAP', FOLD_SOURCE_WORDS),
    ('BUG_STORED_STATES',   'BUG_STATE_WORD_MAP',  FOLD_BUG_STATE_WORDS),
)
WORD_MAPS = {'CAPABILITY_WORD_MAP': str.lower, 'BUG_STATE_WORD_MAP': lambda w: w.replace('-', '_')}
BUG_UPDATE_KINDS = ('bug.opened', 'bug.evidence', 'bug.verdict')   # bugs.updated_at = the latest of these; opened_at = the first bug.opened
# 0.2.179 §3 (G7, G8, G12): capabilities.observed_at = the envelope ts of the latest observation for (repo, accessor);
# run_amendments.actor = the chain envelope's actor subject at that seq; the store's `repo` is org.toml repos[].name, and a
# chain repo string maps through repos[].chain_aliases (a per-org setting in the existing file); reads filter by the org's
# current repo list. TWO RELEASES for the key (A4): the live floor's org.toml reader denies unknown keys (RepoLive and OrgToml,
# code-read at 9e3f379e), so this cut's binary READS the key and the key is WRITTEN only once this cut is live AND is the
# rollback floor. Until then an org whose chain carries a repo string that is neither a repo name nor one of its aliases has
# its backfill DEFERRED (not run for that org: no store rows, no DRIFT pins) and reports once, `backfill.waiting_aliases`
# naming the strings; costless in release 1 (reads stay on the fold). Once org.toml names the aliases, the next boot runs that
# org's backfill. AFTER the backfill, a chain repo string that is neither a name nor an alias IS a DRIFT, reported once, never
# nulled (today's fold nulls it).
# 0.2.179 §4 (G16; G2 for edition-only findings): each table backfills ONCE at boot when its mark is missing, in one store
# transaction, from the same fold functions over the chain in seq order. The source kinds are EXACTLY the kinds the fold
# consumes today (the conductor's FOLD_CONSUMED_KINDS; a cert row holds the two equal): finding.filed and
# finding.followup_published are route-written kinds no fold reads, so they are no backfill source. Every fold step is
# idempotent in seq order; runs, amendments and history skip a seq they already hold.
BACKFILL_RULES = (   # (store.table, key, source kinds, merge rule)
    ('runs.runs',               '(repo, run_seq)',           ('run.stamped', 'run.imported', 'run.amended'), 'insert once; the EFFECTIVE (amended) reading in the columns, the stamp as said in stamped_json; external_counts_json NULL when external_basis_lost; same key + same bytes = nothing; same key + different bytes = DRIFT; never overwritten'),
    ('runs.run_amendments',     '(repo, run_seq, ledger_seq)', ('run.amended',),                            'insert once per applied amendment; actor = the envelope actor'),
    ('runs.capabilities',       '(repo, accessor)',          ('instrumentation.observed',),               'latest wins; words by CAPABILITY_WORD_MAP; observed_at = that envelope ts'),
    ('findings.findings',       'finding_id',                ('finding.curated', 'finding.revised', 'finding.issue_linked', 'finding.disclosure_approved', 'finding.healed', 'finding.repo_attributed', 'finding.reverify_failed', 'finding.proof_repair_pending', 'disclosure.recorded'), 'latest wins per field; proof_repair_pending sets repair = repairing and stamps fixed_rev; state by FINDING_STATE_DERIVATION; revisions = the count of revised history rows (derived, never incremented); lifecycle and disclosed keep their defaults (retired)'),
    ('findings.finding_history','(finding_id, ordinal)',     ('finding.curated', 'finding.revised', 'finding.issue_linked', 'disclosure.recorded', 'finding.healed'), 'ordinal = 1-based position in chain order; ledger_seq = the act seq; served ORDER BY ordinal'),
    ('findings.disclosure_gate','finding_id',                ('disclosure.recorded',),                    'first_disclosed_at = min ts; channels_json and rationales_json = sorted set unions'),
    ('findings.first_served',   'finding_id',                ('edition.advanced', 'disclosure.recorded'), 'earliest wins, never replaced'),
    ('bugs.bugs',               'bug_key',                   ('bug.opened', 'bug.evidence', 'bug.verdict', 'finding.curated', 'disclosure.recorded', 'run.amended'), 'one row per bug; duplicate opens absorbed; state by BUG_STATE_WORD_MAP; opened_at/updated_at by BUG_UPDATE_KINDS; finding_id latest wins through bug_keys; disclosed through the finding'),
)
# 0.2.197 §1 (E82): the subject of a drift pin's kind: the payload keys naming the row it would have fed. A pin's row_key stays the
# malformed record's chain seq (E52), never rewritten; the subject is matched by reading these keys from the pin's fold_json payload
# (plus the envelope `repo` for the runs store's kinds) against the later record's. 0.2.231 §3 (E124 (68), the operator 2026-11-06): the
# BOOT CHECK marks a malformed-record drift pin resolved_by_seq when a later VALID record of the same kind and subject applies; the pin
# stays; no staff resolve route (F-035 resolved by 213850, F-036 by 213851). "Open pins exclude resolved" is a test-oracle rule (no
# conductor reader exists). The kinds are BACKFILL_SOURCES' (the E52-pinned kinds).
DRIFT_PIN_SUBJECTS = (   # (record kind, payload keys naming the subject)
    ('finding.curated',              ('finding_id',)),
    ('finding.revised',              ('finding_id',)),
    ('finding.issue_linked',         ('finding_id',)),
    ('finding.disclosure_approved',  ('finding_id',)),
    ('finding.healed',               ('finding_id',)),
    ('finding.repo_attributed',      ('finding_id',)),
    ('finding.reverify_failed',      ('finding_id',)),
    ('finding.proof_repair_pending', ('finding_id',)),
    ('disclosure.recorded',          ('finding_id',)),
    ('bug.opened',                   ('bug_key',)),
    ('bug.evidence',                 ('bug_key',)),
    ('bug.verdict',                  ('bug_key',)),
    ('run.stamped',                  ('run_seq',)),      # with the record's envelope `repo`, not a payload key
    ('run.amended',                  ('run_seq',)),
    ('instrumentation.observed',     ('accessor',)),     # with the envelope `repo`
    ('run.imported',                 ('run_seq',)),      # with the envelope `repo`
    ('edition.advanced',             ('edition_hash',)),  # findings.first_served; the later valid advance of the same edition resolves it
)
FINDING_STATE_DERIVATION = "m4's precedence (retracted > healed > filed > disclosed > curated) over CHAIN-DERIVED inputs: retracted from lifecycle for curated rows only; healed from the latest finding.healed (fixed_rev); filed from the latest finding.issue_linked; disclosed from a disclosure_gate row; else curated — written directly, with healed_* and issue_* set from the same records"
# 0.2.179 §4 (G2): edition-only findings. A finding disclosed or healed on the chain but never curated on it, and listed in a
# served edition's findings_served (twelve measured: ten on turso, one each on pager-free and supabase-odb-demo), gets ONE row
# with origin = 'edition': title and spec_anchors from the served edition entry that first served it; display_alias = the
# entry's, else the id; evidence_framing = the entry's evidence, else NULL; narrative and severity NULL; first_curated_at =
# last_revised_at = the earliest of its first served receipt's ts and its disclosure's ts; state by FINDING_STATE_DERIVATION.
# The gate, first-served and history rows then insert and the FKs stay. A finding in no served edition and never curated is a
# DRIFT, skipped, reported once. The backfill reads the served edition file from the tenant's books fork.
# 0.2.179 §5 (G10, G11, the collision): schema_meta rows per store. One sequence space per tenant, strictly increasing and NOT
# contiguous (compacted tenants: gaps of 107, 37 and 11); the catch-up applies seq > catchup.seq and never assumes contiguity.
FOLD_RETIREMENT_STORES = ('runs', 'findings', 'bugs')   # the stores the backfill, the outbox and the reconciler keep (drift_pins, pending_audits)
# 0.2.205 §2: the boot catch-up from the chain stays (it also backs the G2 edition-only findings) and the 0.2.179 §5 check stays as
# the drift detector: store rows against a re-fold of the chain, the diff at zero being the charter's gate G1.
# 0.2.189 §4 (E50, E52): what a drift pin may name. DRIFT_PIN_TARGETS = every table of every store (computed from the store
# schemas at generate time, emitted to vocabulary.rs) plus these words that name no store table: `repo_aliases`, the pin of a
# chain repo string that resolves to no repo (row_key = the string).
DRIFT_PIN_EXTRA_TARGETS = ('repo_aliases',)
# 0.2.180 §2 (E40's data home): a third field, the stores a key applies to (each in STORES_LIVE).
STORE_META_KEYS = (   # (key, value shape, the stores it applies to)
    ('backfill.<table>', '{"completed_at": <ts>, "through_seq": <chain seq>}', FOLD_RETIREMENT_STORES),   # present once that table's backfill ran; 0.2.226 §3a (E124 (64)): SUPERSEDED by the per-store `marks` table (read one release, never written again)
    ('catchup.seq',      '<chain seq>', FOLD_RETIREMENT_STORES + ('strands',)),   # 0.2.189 §2 (E48): strands joins: the strand catch-up's mark   # the store's high-water mark: every consumed record with seq > mark is applied at boot, in seq order
    ('front_started_at', '<ts>', ('strands',)),   # 0.2.180 E40: written ONCE at the first boot of the binary that posts from strands.db; a strand opened or last changed before it is history and is not posted
    ('backfill.waiting_aliases', '[<chain repo strings>]', FOLD_RETIREMENT_STORES),   # 0.2.180 (cBackfill's G3 on 0.2.179): the "reported once" home of a deferred backfill (0.2.179 §3): written when an org's backfill defers, cleared when that org backfills
    ('drift.<table>.<column>', '{"reported_at": <ts>, "rows": <n>}', ('findings',)),   # 0.2.181 E41: the boot-mapping DRIFT of a store column (findings.state, 0.2.177 §4), remembered once
    ('backfill.conversations', '{"completed_at", "rows"}', ('strands',)),   # 0.2.182 §4: the one-time copy of campaigns.db's chat tables into conversation strands, per tenant (release 1)
    ('forge_cost_cursor', '<finished_at>:<job_id>', ('credits',)),   # 0.2.207 (d): the last recorded forge job, per org store
    ('agent_cost_unknown:<job_id>', '<ts>', ('credits',)),          # 0.2.207 (f): once-mark, plain RFC 3339 text: the job's cost could not be read; one WARN; no chain kind
    ('pending.config_line', '{"seq": <config-ledger seq>, "rows": [{"table": <editions table>, "key": <row key>, "prior_row": <the row before the write, or null>}, ...]}', ('editions',)),   # 0.2.226 E124 (87): the store-first config writer's outbox: written in the rows' transaction, cleared when the config line at `seq` lands; boot restores every row when no line at seq exists (the §0 one-step rule's repair). 0.2.227 §3 (E124 (88)): ONE entry per config line holding EVERY row the line moved (an image request marks both kinds; rows may span tables); prior_row null = the row was inserted (boot deletes it); seq is never repeated
)
# The outbox (pending_audits, every fold-retirement store): a write route writes its row AND the audit record it intends in
# the same store transaction; the appender posts the record with its idem, stores the seq on the row, deletes the entry. The
# idem is the ROUTE's semantic key where it has one (disclose:{org}:{repo}:{id}, instr:{repo}:{pr}:{accessor},
# fix-candidate:{fid}:{sha}, the dispose and capability keys), else sha256(record_json || written_at); never the store key or a
# bare record hash. A route checks its idem against the outbox and the chain's idem map BEFORE the store write. The boot
# reconciler, in order: (i) append every pending_audits entry, store the seq, delete it; (ii) merge every consumed record with
# seq > catchup.seq by BACKFILL_RULES; (iii) a key where store and fold still disagree is a DRIFT: one drift_pins row, reported
# once, never overwritten; then catchup.seq moves to the chain head. 0.2.189 §4 (E50, E52): a pin's table_name is a
# DRIFT_PIN_TARGETS word; drift_pins.store_json is NOT NULL, so a pin with no store side carries the JSON text 'null' (the four
# characters), never SQL NULL. An unresolved chain repo string is pinned (repo_aliases, row_key = the string, store_json 'null',
# fold_json {"repo": <the string>, "kinds": [...]}); a chain record the fold cannot apply is pinned once (the table it would
# have fed, row_key = the record's seq, store_json 'null', fold_json = the record), never silently skipped: the 0.2.179 §7
# cert row reads 156 (not 158) for turso's finding_history, with two pins. A runs row whose outbox entry is pending and whose
# (repo, run_seq) a floor-minted chain record now holds is RE-MINTED (the next free seq; the idem changes only when the record
# has no route key), never dropped and never a DRIFT. The store-vs-fold diff by key runs at boot and after each refold.
# 0.2.179 §6 (G14, G15): editions.db, the advances and the track positions stay in the fold in release 1; runs.edition_anchor
# is the run's own fact. Release ordering (§3): repos[].chain_aliases is written to an org's org.toml only once this cut's
# binary is live AND the rollback floor; the two measured orgs (turso, mesa-dot-dev) backfill at the boot after that write; the
# deploy block carries the floor's deny_unknown_fields reading as a premise.
# 0.2.167 §2: the credits store's words. A new paying feature is one CREDIT_SPENDERS row and one reason.
# 0.2.188 §2: two reason lists. CREDIT_REASONS is every word an agent_costs row may carry (agent_costs.reason names it): the
# five drawing reasons and maintenance, heal, pipeline and agent, which draw nothing (an Aretta-paid row is credits_charged 0;
# no CREDIT_SPENDERS row names them). CREDIT_DRAW_REASONS, a subset, is credit_ledger.reason's words (its CHECK since credits
# migration 1 is exactly this set; no migration).
CREDIT_REASONS = ('launch', 'chat_message', 'grant', 'adjustment', 'refund',
                  'maintenance', 'heal', 'pipeline', 'agent', 'image_bake')   # 0.7b adds its agent reason as one word; 0.2.202: image_bake
CREDIT_DRAW_REASONS = ('launch', 'chat_message', 'grant', 'adjustment', 'refund')   # the credit_ledger.reason CHECK reads it
# 0.2.169 erratum E4 (cB groundwork 77bec10f: chat's cost is known only after the answer): a `when` column, ('before',
# 'after'). A post-paid ('after') spender refuses `insufficient_credits` when the balance is not above zero before the act,
# acts, then draws under the same unique key; the draw is min(cost, balance): the balance NEVER goes below zero (#616), a
# shortfall is absorbed, not owed, and the next post-paid act is refused while the balance is zero.
CREDIT_SPENDERS = (                   # (reason, the act that draws, the ref it carries, when) — a new paying feature is one row and one reason
    ('launch',       'campaigns.cards.launch', 'campaign id',     'before'),   # pre-paid: draw, then act; refund on a failed act
    ('chat_message', 'the chat turn',          'chat message id', 'after'),    # post-paid: check the balance is above zero, act, then draw the actual cost
)
# 0.2.167 §3, THE DRAW RULE (every PRE-PAID spender, `when` 'before'; the four steps, each one transaction on one store): mark_pending in the
# spender's store; draw in credits.db (check the balance, append credit_ledger {delta: -cost, reason, ref, draw_key:
# '<reason>:<ref>:<attempt>'}, update credit_pool, in ONE transaction; an existing draw_key returns its row and writes nothing; a
# balance short of the cost refuses `insufficient_credits` 402); act; mark_done. A failed act appends a refund {delta: +cost,
# reason: refund, ref, refund_of: <draw seq>, draw_key: 'refund:' + the draw's key = 'refund:<reason>:<ref>:<attempt>'} (at most
# one per draw) and returns the spender's row to the state before mark_pending (a campaign stays `launchable`, #616).
# 0.2.169 erratum E5: a pre-paid key carries the ATTEMPT, `attempt` = 1 + the number of refunded draws for that ref, so a
# relaunch after a refund draws a fresh key and every attempt's refund is unique under UNIQUE(draw_key); a retry of the SAME
# attempt still finds its key and writes nothing.
CREDIT_DRAW_STEPS = ('mark_pending', 'draw', 'act', 'mark_done')   # 'refund' on a failed act
# the tuples state/stores/vocabulary.json mirrors, in its order (scripts/generate-stores.py writes it from these)
STORE_VOCABULARY = ('TOPIC_KINDS', 'STRAND_KINDS', 'STRAND_ACTIONS', 'STRAND_STORED_STATES', 'STRAND_STATES', 'STRAND_TERMINAL', 'FRONTS',
                    'SURFACES', 'RETENTION_CLASSES', 'RUN_LINES', 'ACCESSOR_STATES', 'OBSERVATION_SOURCES',
                    'FINDING_STATES', 'FINDING_ACTS', 'DISCLOSURE_CHANNELS', 'BUG_SOURCES', 'BUG_DISPOSITIONS',
                    'BUG_STORED_STATES', 'BUG_STATES', 'BUG_VERDICTS', 'EDITION_SLOTS',
                    'STRAND_CLOSED_REASON_WORDS',   # 0.2.220 §8.3: strands.db strands.closed_reason
                    'CREDIT_REASONS',   # 0.2.167: credits.db credit_ledger.reason; 0.2.188: agent_costs.reason
                    'CREDIT_DRAW_REASONS',   # 0.2.188 §2: credits.db credit_ledger.reason
                    'PRESS_ACTIONS',    # 0.2.172 §15.5: strands.db pending_presses.action
                    'DEFERRED_WRITE_ARMS',   # 0.2.175 E19: findings.db deferred_writes.arm
                    'AGENTIC_PAYERS',   # 0.2.176 §5b: credits.db agent_costs.payer
                    'CAMPAIGN_STORED_STATES', 'CAMPAIGN_STATES',  # 0.2.177 §3; 0.2.220 §8.3: campaigns.db campaigns.status names the stored set
                    'FINDING_VISIBILITY_STORED', 'FINDING_LIVENESS_STORED', 'EDITION_STATES', 'EDITION_CANDIDATE_VERDICTS',   # 0.2.220 §8.3: findings.db's three columns
                    'TRACK_STORED_STATES', 'TRACK_ADVANCE_WORDS', 'IMAGE_STORED_STATES', 'IMAGE_KINDS', 'IMAGE_BAKE_FAILURES',   # 0.2.222 §1, §2: editions.db tracks and images
                    'IMAGE_BUILT_STATES', 'IMAGE_BAKE_REASONS',   # 0.2.226 §2: editions.db bakes (m7)
                    'BACKFILL_MARK_STATES', 'BACKFILL_HOLD_REASONS',   # 0.2.226 §3a (E124 (64)): every marked store's marks table
                    'TRACK_POSITION_SOURCES',   # 0.2.226 E124 (83): editions.db tracks.position_from
                    'CARD_SOURCES',     # 0.2.190 §1 (E68.1): campaigns.db campaigns.source
                    'TRACK_KINDS',      # 0.2.179 §2: runs.db runs.track_kind
                    'RUN_PURPOSES',     # 0.2.189 E65: runs.db runs.purpose
                    'FINDING_ORIGINS',  # 0.2.179 §2: findings.db findings.origin
                    'STRAND_AUDIENCES',   # 0.2.176 §3: strands.db strands.audience
                    'ACTION_PLANES')      # 0.2.191 E73: strands.db actions.plane
# The migration runner's own table in every store (not a migration's): `migration` = the highest applied.
SCHEMA_META_SQL = 'CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)'
# 0.2.165 erratum E7 (the door, STATUS 2026-10-24T01:36:36Z): after the Slack link callback the browser lands on
# `302 /#/account?code=<code>` with `code` in LINK_LANDING_CODES; `detail` is never in the redirect. All are existing codes.
LINK_LANDING_CODES = (
    'wrong_workspace',     # the Slack account belongs to a workspace that is not this org's bound one
    'alias_taken',         # that Slack account is already linked to another person of this org
    'slack_denied',        # Slack answered ok:false to the exchange
    'oauth_unconfigured',  # the Slack OAuth provider is not configured
    'not_bound',           # the org's Slack binding was gone between start and callback
    'unauthenticated',     # no session at the callback: a machine credential, or a revoked person
    'store_unwritable',    # the identity store could not be written; no audit row is appended
)
# 0.2.165 erratum E8 (the door, DECISIONS 2026-10-24T02:12:55Z; measured by certB at 62b74176, I3): the link START row,
# reached by browser navigation, answers a BROWSER caller (Accept naming text/html) with the callback's landing shape,
# `302 /#/account?code=<code>`; a non-browser caller keeps the JSON refusal. The only handler refusals a browser can reach.
START_LANDING_CODES = (   # a subset of LINK_LANDING_CODES
    'not_bound',          # the org has no Slack binding
    'oauth_unconfigured', # the Slack OAuth provider is not configured
)
# 0.2.169 erratum E8 (the door, DECISIONS 2026-10-24T17:17:47Z): the operator added `users:read` to the Slack app the install
# route requests (`notify.install.redirect`; the notify client, Slack app "Aristo Agent"; "Aretta Code" is a different app and
# is not touched). The fixed scope string is data; the customer install operation and the admin install callback carry
# `x-scopes` held equal to it. With users:read, `slack_name` is served live (users.info), never stored. The Slack front
# (0.2.172 after the door's merge-order renumbering; 0.2.175 erratum E9) added channels:history.
# 0.2.172 §8 (§15.3): `channels:history` joins for the thread replies (message.channels); `groups:history` joins as one word in
# the cut that first binds a private channel (a fixed scope string carries no condition).
SLACK_INSTALL_SCOPES = ('chat:write', 'channels:read', 'groups:read', 'app_mentions:read', 'users:read', 'channels:history')   # the install route's fixed scope string, in order; the install callback refuses `slack_scope_missing` when Slack grants less
# 0.2.171 (box job KINDS; the operator 2026-10-23T21:43:19Z "one slot reserved for CI, more generally we need a spec for
# different kinds of box jobs"; vyasa's FINAL SPEC-0.2.171-box-job-kinds.md §1): a box job's kind, as a closed list. Today the
# job record has no kind field; the floor DERIVES it from `spec.source` and `spec.task.type` (the derivation column below,
# box f715f04e). `kind` becomes a DECLARED JobSpec field: release 1 accepts it optional and, when absent, derives it by that
# mapping; release 2 requires it (§A4). 0.2.175 erratum E13: forge `ci` → ci; `None` → verify; EVERY other source, named or
# not (forge `issue`, forge `publish`, `memory-reflect:<tenant>`) → agent; no new word, and `agent` is non-CI for the reservation.
BOX_JOB_KINDS = (
    # (kind, what it is, today's derivation (the floor's mapping), task.type, who dispatches it)
    ('ci',           'a PR head\'s CI run',                       'spec.source forge kind ci',    'command', 'the forge dispatcher (webhook, HMAC)'),
    ('agent',        'every other job: an issue, a publish, a memory reflect', 'spec.source any other', 'prompt',  'the forge dispatcher (webhook, HMAC); the CLI (memory-reflect)'),
    ('verify',       'the conductor\'s verification run',         'spec.source None',             'command', 'the conductor (its verify rows: `verify` · repo)'),
    ('conversation', 'a 0.7b chat container (specified, unbuilt)', '—',                            'prompt',  'the conductor (0.7b)'),
    # 0.2.202 §1: a seat's image accept dispatches the bake as a box job (tolerate-first: the kind ships before the dispatch)
    ('image_bake',   'the bake and probe of an org\'s accepted image request, both kinds', 'spec.source None', 'command', 'the conductor (live.image.accept)'),
)
# §2 (RULED, the operator): of the per-tenant cap (the org's quotas.max_concurrent, else min(4, max_slots)), ONE slot only a
# `ci` job may hold. A tenant's non-ci jobs hold at most cap − reserved(ci); ci jobs up to cap; FIFO by (created_at, id)
# within a kind; no other priority; CI supersede stays. A tenant with cap 1 cannot reserve (the box clamps per tenant).
# 0.2.175 erratum E12: the per-tenant cap when the org sets no quotas.max_concurrent (min(it, max_slots)); the box's
# DEFAULT_TENANT_MAX_CONCURRENT (model.rs:83) as data, emitted into the box's diagnostics.rs beside BOX_JOB_KINDS
BOX_DEFAULT_TENANT_CAP = 4
# 0.2.219 (box disk bounds; the operator's defaults 2026-11-02; mechanism b2 settled by ops' host read): ONE fixed-size volume per job
# (the three job mounts; ENOSPC inside it, never a kill; [limits] job_volume_bytes in org.toml may LOWER it, never raise it: min(org, box);
# a JobSpec never raises it) and a host disk watermark read each schedule pass on DISK_WATERMARK_FILESYSTEMS: if EITHER is at or above the
# percent, every queued start holds in its existing non-cancelling wait with diagnostic `disk_low`; running jobs continue; clears below
# percent - DISK_WATERMARK_HYSTERESIS_PERCENT. Boot: max_slots x job_volume_bytes must fit below the watermark on the docker root, else
# one WARN (never a refusal); a driver_host on another machine: one WARN, volumes unbounded, shown in health. The health `disk` is ONE
# object, the worse of the two filesystems. The conductor polls the box's /v1/health every [verify] box_health_poll_seconds and posts the
# two box_health POSTABLE rows (the post is the record: no ledger event, no platform store).
JOB_VOLUME_BYTES_DEFAULT = 53687091200          # 50 GiB: control.job_volume_bytes when the box config sets none
DISK_WATERMARK_PERCENT_DEFAULT = 85             # control.disk_watermark_percent when the box config sets none
DISK_WATERMARK_HYSTERESIS_PERCENT = 5           # the hold clears below watermark - this
DISK_WATERMARK_FILESYSTEMS = ('docker_root', 'control_db_parent')   # the two filesystems sampled each schedule pass (DockerRootDir; the parent of control.db_path)
DISK_WATERMARK_STATES = ('below', 'above')      # health disk.watermark; `above` if either filesystem is
BOX_HEALTH_POLL_SECONDS_DEFAULT = 30            # [verify] box_health_poll_seconds when conductor.toml sets none
BOX_RESERVED_SLOTS = (('ci', 1),)
# 0.2.202 §1b (walk 2 finding 1-4): the box queue. At most BOX_KIND_HOST_CAPS jobs of a kind per jobs host at a time (the bake
# lock); an image_bake takes a normal non-ci slot of the tenant's cap (BOX_RESERVED_SLOTS unchanged) and does not wait for an
# idle host; its priority is BOX_KIND_PRIORITIES' sentence.
BOX_KIND_HOST_CAPS = (('image_bake', 1),)
BOX_KIND_PRIORITIES = (('image_bake', 'ahead of agent, verify and conversation within the tenant'),)     # of the per-tenant cap, one slot only a `ci` job may hold (the operator's ruling)
# 0.2.171 erratum §11 (defect, bau-fde E9 #314): the publication hold gates EVERY customer-outbound write, not only the
# publisher's mirroring (a held org still had its issue reopened and commented by a heal, bauplanlabs 10-24 01:39:44Z). A
# person's act is refused 409 `publication_held`, nothing written (a strand accept on it is the ordinary `act_failed`,
# reason publication_held); an in-process writer defers: recorded, written once in order when the hold clears.
PUBLICATION_HOLD_GATES = (    # (writer, what it does while the org is held)
    ('findings.finding.disclose',      'refuse'),   # a person's act: 409 publication_held, nothing written; 0.2.224 §6b (E124 (54)): replaces the retired findings.issue.create row (0.2.221 §6: the disclose publishes upstream at once)
    ('findings.repair.reject',         'refuse'),
    ('findings.upstream_pull.publish', 'refuse'),
    ('findings.pull.publish',          'refuse'),
    ('heal.outcome_write',             'defer'),    # the heal's reopen and comment: recorded, written when the hold clears
    ('publisher.mirror',               'defer'),    # the publisher's mirroring, as today
)
# 0.2.172 §6: an unlinked member's press is remembered in strands.db `pending_presses` (migration 4; transient; one row per
# alias and strand, a newer press replacing the older) for this many minutes; a link within it replays the press as the newly
# linked person through the ordinary strand route; expired rows are swept on read and at the notify tick.
PENDING_PRESS_TTL_MINUTES = 10
# 0.2.172 §15.5: the customer actions a press can carry (strands.db pending_presses.action's CHECK; each a
# STRAND_CUSTOMER_ACTIONS word; `comment` is never a press: a thread reply is a comment, §7)
PRESS_ACTIONS = ('accept', 'reject', 'defer')
# 0.2.211 §2 (cSlackBtn #757, M-a/M-b): an internal strand root carries the kind's buttons as a customer root does (buttons =
# kind_actions); the two internal kinds' press labels (the dashboard's UI_PRESS map keys them; approved AA7/AA8); no `defer` on an
# internal kind; the reject carries one REJECT_REASONS word (a reason-less reject is refused, never a panic). An internal press
# value names the org (one internal channel serves every org), admitted only from a `[[notify.internal]]` channel; the customer
# grammar of 0.2.172 §2 is unchanged. The presser's alias resolves only in an internal org whose ENABLED internal-bound Slack team
# equals the payload's team (0.2.172 §6 with the revocation fold); the act runs on the strand's org as that person through the
# strand route (steer), recorded with surface `slack`; the root is edited in place at once. Unmapped: refused with the private
# unlinked answer and a Link button on the internal org bound to the press's workspace (else the first internal org, by slug),
# `slack.declined` on the strand's org, NOT remembered (pending_presses unchanged).
UI_PRESS_INTERNAL_LABELS = (('edition_promote', 'ui.strand.promote_edition'), ('track_move', 'ui.strand.move_commit'), ('triage', 'ui.strand.triage_verdict'),
                            ('image', 'ui.strand.accept_image'))   # 0.2.221 §2; 0.2.222 §2 (E124 (37) 6): the image accept
INTERNAL_PRESS_VALUE = '<org>/<strand_id>:<version>[:<reason>]'
# 0.2.172 erratum §13 (cStrands, §A6): a retire's reason is a closed word (never free text on the chain). Optional on the
# retire row, defaulting to retired_by_aretta; the system's own retires name theirs. 0.2.178 §5 (E37): `discarded` retires
# (release 1: CAMPAIGNS_BOOT_RULES map its rows by data), so `campaign_discarded` leaves; the 2026-10-24T21:40:05Z ruling is
# superseded: such a card is `withdrawn` and its strand retires with campaign_withdrawn.
RETIRE_REASONS = (
    'finding_retracted',     # the system: the finding's state became `retracted` (0.2.177 §4: FINDING_STATES)
    'pull_closed_unmerged',  # the system: the upstream PR closed without a merge
    'campaign_withdrawn',    # the system: the card became `withdrawn`
    'campaign_retired',      # the system: the card became `retired` (the org's campaigns closed)
    'retired_by_aretta',     # an Aretta actor's `strands.strand.retire`
    'track_accepted',        # 0.2.222 §1: the system: the battery accepted the requested track (the track strand closes with no press)
)
# 0.2.182 §1 (G6, split): the fixed reason an `end` act carries (the customer ended the conversation). NOT a RETIRE_REASONS
# word: an Aretta retire never sends it (StrandReason's enum stays RETIRE_REASONS; `ended` there -> 400).
END_REASON = 'ended'
STRAND_CLOSED_REASON_WORDS = STRAND_CLOSED_REASONS + RETIRE_REASONS   # 0.2.220 §2: every word closed_reason may carry (derived)
# 0.2.173 §1/§5 (one meaning per word): `subject` names a PRINCIPAL's id (`github:alice`, `seat:fde`, `service:conductor`) and
# nothing else; `handle` is the bare provider name; a strand is about a TOPIC. SUBJECT_FIELDS lists every property and store
# column named with `subject` (file, path; `[]` = an array's items); validate holds it EQUAL to what the schemas carry, so a new
# `subject` field is a deliberate row here. OrgWhoami.subject is the deprecated bare handle (until the next major).
SUBJECT_FIELDS = (
    ('state/credentials/credential.schema.json',             'principal_subject'),
    ('routes/admin/schemas/Access.schema.json',              'subject'),
    ('routes/admin/openapi.yaml#LiveState',                  'access[].subject'),
    ('state/live/org.schema.json',                           'access[].subject'),
    ('state/ledger/configevents.schema.json',                'access[].subject'),   # 0.2.232 §3: platform.changed carries org.toml's access list as written
    ('state/live/conductor.schema.json',                     'staff[].subject'),
    ('routes/admin/schemas/PlatformTokenMinted.schema.json', 'subject'),
    ('routes/admin/schemas/PlatformTokenRow.schema.json',    'subject'),
    ('routes/customer/schemas/OrgCliTokenRow.schema.json',   'subject'),
    ('routes/customer/schemas/WireSlackLinks.schema.json',   'links[].subject'),
    ('routes/customer/schemas/WireCredits.schema.json',      'subject'),
    ('routes/customer/openapi.yaml#OrgWhoami',               'subject'),   # deprecated: the bare handle; read `handle`
    ('routes/admin/schemas/StaffWhoami.schema.json',         'subject'),   # 0.2.223 §2: the staff whoami's person, `github:<login>`
    ('state/stores/strands/schema.json',                     'pending_presses.alias_subject'),  # the presser's alias subject
    ('state/stores/strands/schema.json',                     'participants.subject'),   # 0.2.182: who has joined a conversation
)
# 0.2.177 errata E22, E23 (cB, measured at 8203d12a on a live-store copy): 0.2.173 §4's bare-login close is THREE releases:
# release 1 reads both forms and keeps writing the bare login (writing the subject there hides a person's chats from a
# rolled-back binary); release 2 writes `github:<handle>` and runs the value migrations; release 3 drops the dual read.
BARE_LOGIN_CLOSE = ('read_both', 'write_subject_and_migrate', 'drop_dual_read')
# The release-2 value migrations rewrite by MEMBERSHIP, not by shape. (store, table, column): PERSON_SUBJECT_COLUMNS are the
# columns a read MATCHES a person against, and are migrated; chat_messages.session_id holds the subject (campaigns migration
# 010). ATTRIBUTION_COLUMNS no read matches, and are NOT migrated: the writer stores the subject from release 2 on, old rows
# keep what they hold, and the rendering rule shows either form. campaigns.estimated_by is display text and in neither.
PERSON_SUBJECT_COLUMNS = (
    ('campaigns', 'campaigns',      'suggested_by'),
    ('campaigns', 'conversations',  'subject'),
    ('campaigns', 'chat_clears',    'subject'),
    ('campaigns', 'chat_deletions', 'subject'),
    ('campaigns', 'chat_messages',  'session_id'),
    ('campaigns', 'credit_ledger',  'subject'),
    ('credits',   'credit_ledger',  'subject'),
)
ATTRIBUTION_COLUMNS = (
    ('campaigns', 'campaigns',         'author'),
    ('campaigns', 'campaigns',         'finalized_by'),
    ('campaigns', 'campaigns',         'launched_by'),
    ('campaigns', 'campaigns',         'status_set_by'),
    ('campaigns', 'campaigns',         'declined_by'),
    ('campaigns', 'chat_messages',     'author'),
    ('campaigns', 'label_display_map', 'author'),
)
# In a PERSON_SUBJECT_COLUMNS column a bare value is rewritten to `github:<value>` only when it is not a machine word, measured
# on the live stores before the migration (the census is part of its proof; a value in neither set stops the migration).
# Machine words keep their bare form until their own rename (the operator token's retirement).
MACHINE_BARE_VALUES = ('operator', 'op')
# 0.2.174 (the strand is one timeline): a conversation is one `strands` row and one `actions` row per act, in ordinal order;
# strand_versions and comments are folded into actions (migration 6). The acts that carry a message's text: Aretta's on a
# propose or revise (text, options_json, basis_json), a comment's on a comment (text, retention_class); every other act's text
# columns are NULL.
TIMELINE_TEXT_ACTS = ('propose', 'revise', 'comment')
# 0.2.175 erratum E19: a deferred customer-outbound write has a durable record. The heal's two outbound arms, deferred while
# the org is held (PUBLICATION_HOLD_GATES heal.outcome_write), are kept in findings.db `deferred_writes` (migration 3) with the
# inputs not on the chain; the deferral appends the existing publish.blocked {finding_id, arm, reason: publication_held}; the
# release runs the arm in deferred_at order, sets released_at, and the row is swept.
DEFERRED_WRITE_ARMS = ('heal_green', 'heal_red')
# 0.2.176 (the edition pipeline; the operator's cadence rulings, DECISIONS 2026-10-25): what makes an edition worth promoting
# (an upstream move alone runs the readiness battery only), the promote's guards (checked by books.edition.promote itself,
# whoever calls: each refuses 409 with its code, `detail` the evidence), and the pipeline: each step fires on its event's
# record (the conductor is the clock), runs as the org's seat when its gate is a rule, and opens an internal strand when it
# is a press. No step starts inside a deploy window; a run lost to a restart re-fires once.
EDITION_TRIGGERS = ('released_set_changed', 'fix_for_open_finding')   # what makes an edition worth promoting; an upstream move alone runs the readiness battery only
PROMOTE_GUARDS = (        # (guard, refusal code); checked by books.edition.promote itself, whoever calls. 0.2.178 §2: each comment is the guard's data
    ('candidate_exists',     'promote_no_candidate'),       # a completed run whose box job names exactly this edition
    ('candidate_conclusive', 'promote_inconclusive'),       # that run finished; an infra death is not a verdict (one re-fire, INFRA_REFIRE_BUDGET)
    ('no_measurement_loss',  'promote_measurement_loss'),   # no spec in the COMPARISON_UNIVERSE is PASS or VIOLATED on SERVED_RUN and GATED_OUT, INFRA or absent on the candidate
    ('no_regression',        'promote_regression'),         # no spec in the COMPARISON_UNIVERSE is PASS on SERVED_RUN and VIOLATED on the candidate
    ('failures_explained',   'promote_unexplained_failure'),# every VIOLATED spec on the candidate is in findings[].specs of a finding whose findings.db state is any FINDING_STATES word but `healed`
    ('cut_pr_green',         'promote_cut_unverified'),     # the candidate's edition.cut exists, is merged, and every CUT_PR_CHECKS name is present and 'passed'
    ('from_is_served',       'promote_from_moved'),         # fork main's tip equals the candidate's edition.cut.from_commit
)
# 0.2.178 §2 (E24, E25, E27, E36): loss and regression are computed AT PROMOTE TIME against SERVED_RUN, never a recorded
# baseline (a served edition changed since the candidate run means the new served run is used); a spec outside the
# COMPARISON_UNIVERSE is neither PASS nor lost; `detail` names `served_run` and the specs. INFRA on a spec counts as loss. A
# finding explains a failure in any state but `healed` (`retracted` IS the by-design case); a VIOLATED spec whose finding has no
# findings.db row is UNEXPLAINED (`detail` names the finding id) -- so the fold-retirement backfill (0.2.179) lands before
# failures_explained is enforced: a release-ordering fact for the door, not a switch in the guard. cut_pr_green: no edition.cut
# (cut before the record existed) refuses and is re-cut; a missing CUT_PR_CHECKS name refuses, `detail` naming it.
# from_is_served: `detail` = {main_tip, from_commit}; the promote request gains no field (0.2.177 E27's books_commit reading
# is withdrawn: the promote fast-forwards main). A rollback is recognised by data (the receipt), never by caller.
SERVED_RUN = 'the latest completed run of the served edition (the book\'s latest edition.advanced receipt) whose sut_commit equals the candidate run\'s sut_commit: its external run if one completed at that commit, else its candidate-kind run at that commit; none = promote_basis_pending (409) and the served_remeasure dispatch'   # 0.2.212 §2 (row 1a): like is compared with like; upstream drift never reads as an edition regression
SERVED_RUN_BASIS = 'same_sut'   # 0.2.212 §2: SERVED_RUN is the served edition's latest completed run at the CANDIDATE run's sut_commit; governs BOTH comparison guards (no_regression, no_measurement_loss); the clock's regression alarm compares to the previous served edition, unchanged
# 0.2.212 §2 (Q4): refusals evaluated BEFORE the guard walk on a non-rollback promote; never a PROMOTE_GUARDS row, never seen by
# ROLLBACK_GUARDS (a rollback compares nothing). promote_basis_pending: no SERVED_RUN at the candidate's sut_commit; the strand's
# basis_guards mark no_regression and no_measurement_loss as not passed with this detail, and the served_remeasure dispatch runs.
PROMOTE_PRECONDITIONS = ('promote_basis_pending',)
COMPARISON_UNIVERSE = {'external': 'released', 'internal': 'all'}   # the specs compared, by SERVED_RUN's kind: the served edition's released specs, or all its specs
# 0.2.177 E28: a rollback is the promote of an edition that already has an edition.advanced receipt for this book (recognised by
# that receipt, never by caller); it returns to a measured, previously served state, so only its own run is checked.
ROLLBACK_GUARDS = ('candidate_exists', 'candidate_conclusive')
# 0.2.214 §3 (arc-verify slice 2; the conductor's rows): ONE predicate, two uses. An ARC RUN is a run whose run.stamped `git_ref`
# parses as Arc(slug) (0.2.213: refs/heads/dev-<slug>) with `track` None; the projection rule "git_ref both-or-neither with track" is
# REPLACED by: `track` set => `git_ref` set; `git_ref` set with `track` None => the ref parses as Arc(slug). An arc run stamps line dev,
# external 0, purpose customer, basis_only 0. The two reasons a run is NOT a basis are BASIS_EXCLUSIONS; each selector below applies
# the exclusions its row names. Ruling (product-v2, accepted): an arc run does NOT anchor an edition (the `edition_anchor` row); the
# stored edition_anchor column's rule is the product's. `basis_only` (0.2.212) is unchanged. §1: VerifyCreateReq.arc_ref (NOT v2's
# `ref`: it collides with the run wire's git_ref key); sut_commit stays the required full 40-hex arc tip (R23). §4: the candidate
# session id (the dedupe key) includes the arc slug only when arc_ref is present; a same-arc rerun at the same commit needs `force`.
BASIS_EXCLUSIONS = ('arc', 'basis_only')      # the two reasons a run is not a basis: an arc run (git_ref parses as Arc), a basis_only run (0.2.212)
SELECTOR_EXCLUSIONS = (                       # (selector, the exclusions it applies)
    ('newest_run',               ('arc', 'basis_only')),
    ('edition_basis_run',        ('arc', 'basis_only')),
    ('measured_by',              ('arc', 'basis_only')),     # the clock's battery result
    ('candidate_run_rollback',   ('arc', 'basis_only')),
    ('served_run',               ('arc',)),                  # 0.2.212: the two comparison guards' SERVED_RUN
    ('candidate_run',            ('arc',)),                  # non-rollback
    ('auto_promote',             ('arc',)),                  # the clock's rule-gated promote
    ('regression_base',          ('arc',)),                  # the clock's alarm
    ('edition_anchor',           ('arc',)),                  # the S25 anchor: an arc run never anchors an edition
)
# 0.2.178 §1 (E32): the checks the cut PR must pass, by commit-status context name on its head; the scan runs AS the `scan`
# check and fails when it finds anything. edition.cut.checks holds one entry per name.
CUT_PR_CHECKS = ('ci', 'scan')
# 0.2.187 §4 (G4, G5): the cut is OBSERVED on the fork, not run by the conductor; a cut PR is identified by its head branch,
# `cut/<edition_id>`; a PR on any other branch is not a cut. On its merge the conductor writes edition.cut (naming its repo).
CUT_PR_BRANCH = 'cut/<edition_id>'
# 0.2.187 §1 (G2): the deploy window is a platform-level marker file, set and cleared by the deploy wrapper (routes.md §3's
# declared CLI entry point). No PIPELINE_STEPS step STARTS while the marker exists; a step already running finishes; a run
# lost to the restart re-fires once (INFRA_REFIRE_BUDGET). A marker older than DEPLOY_WINDOW_MAX_MINUTES is stale: ignored
# and reported once.
PLATFORM_FILES = (   # (path, class, writer, durability) — the deployment's own files, beside TENANT_FILES
    ('state/deploy-window', 'derived', 'the deploy wrapper: created before its gate, removed when the new conductor answers /healthz', 'none'),
    ('state/installations/pending/<installation_id>.json', 'facts', "the conductor, on the App's installation webhooks", 'backup'),   # 0.2.195 §1
)
DEPLOY_WINDOW_MARKER = 'state/deploy-window'
DEPLOY_WINDOW_MAX_MINUTES = 60
PIPELINE_STEPS = (        # (step, trigger event, gate, actor, pausable[, basis_only]); 0.2.212 §1: pausable = the step is a DISPATCH the per-repo clock pause holds (the rollback alarm reads and records, starts nothing: false); §2: basis_only (the served_remeasure row only) = its run is stamped basis_only and read only as the comparison guards' SERVED_RUN
    ('readiness_battery', 'upstream_moved | requested', 'rule',  'seat', True),   # 0.2.225 §3b (E124 (60)): one step, two causes: track.upstream_moved and track.requested (the request battery at the resolved commit); the one-battery-per-commit rule (0.2.224 §4, BATTERY_RULES) applies; no new step word
    ('harness_fix',       'readiness_battery_failed',  'rule: merge when green and run-proven', 'seat', True),   # 0.2.190 §2 (E68.2): on a customer-requested sync (E64) the step's first part is the maintenance card: the rule authors AND finalizes it (source MAINTENANCE_CARD_SOURCE, estimated_by MAINTENANCE_ESTIMATED_BY, the estimate MAINTENANCE_ESTIMATE in whole credits, estimated_duration [maintenance] estimated_duration, words MAINTENANCE_CARD_WORDS), and the fix runs as the campaign's agent job on the customer's launch; on a heal-driven sync the step runs the fix directly, payer aretta (0.2.176 §5)
    ('track_move',        'readiness_battery_green',   'press: track_move strand', 'presser', True),
    # 0.2.212 §2 (row 7): on a track.reset record (the track_move strand's accept through mirrors.track.reset; the word `cut` reads) a
    # candidate-kind verify of the SERVED edition at the moved track's commit (edition_anchor = the served edition, sut_commit = the new
    # commit, line dev, purpose customer, external 0), once per (served edition, commit); its result is the SERVED_RUN for the next
    # promote; no customer post; payer JOB_PAYERS `pipeline` (aretta), job_kind verify. The same dispatch runs on a promote_basis_pending refusal.
    ('served_remeasure',  'track_moved',               'rule',  'seat', True, True),
    ('cut',               'track_moved | released_set_changed', 'rule', 'seat', True),
    ('candidate_verify',  'cut_merged',                'rule',  'seat', True),
    ('promote',           'candidate_green',           'rule when customer_visible_change is false, else press: edition_promote strand', 'seat | presser', True),
    ('external_verify',   'promoted',                  'rule',  'seat', True),
    ('rollback',          'external_regressed',        'press', 'presser', False),   # 0.2.212 §1: the regression alarm is exempt from the pause (it starts nothing). 0.2.189 §5 (E55): release 1: the FDE reverts SERVED on the fork and promotes; the clock ALARMS (edition.regressed). The operator's "rollback: automatic" is DEFERRED (ruling 2026-10-27): later it rides an internal strand whose revert action carries the gate: rule|press shape
    # 0.2.218 §1 (operator 2026-11-02: "yes automerge"): records land on the tenant's queue repo main by this step. On each clock tick, when
    # the org's switch RECORDS_AUTOMERGE_SWITCH is on, the conductor (actor `service`: its own [books] bot on the queue repo, no new
    # credential kind) takes the RECORDS_MERGE_PER_PASS oldest open task/* PRs, posts the status RECORDS_MERGE_CHECK on each head (passes
    # iff RECORD_SHAPE_RULES hold), merges head-bound (the sha the check saw) in the cut_merge shape and deletes the branch; a skip names a
    # RECORDS_MERGE_SKIP_REASONS word. One queue.record_merged {pr, branch, head} per merge and one queue.record_skipped {pr, reason} per
    # skip (observation tier). A record reaches main on the machine check alone; a person reviews AFTER. Pause key: the queue repo's NAME
    # in [limits] clock_paused (the SUT repo's pause does not hold it). Protection (errata v5): A now, forge-side: push + merge whitelist =
    # the bot + the operator; B at this cut's deploy, a sealed hand step: the required status context RECORDS_MERGE_CHECK, the person off
    # the whitelist. No verify run: basis_only False.
    ('records_merge',     'tick',                      'rule',  'service', True, False),
)   # 0.2.220 §3 (E122.41): heal_redrive leaves: the heal is webhook-caused (HEAL_TRIGGERS); the clock pause stays as data
RECORDS_AUTOMERGE_SWITCH = 'records_automerge'     # org.toml [switches] key; default OFF; the step fires only when on; per org, turso first
RECORDS_MERGE_CHECK = 'aretta/records-schema'      # the status context the step posts on the PR head before merging; errata v5 B: the required context
RECORDS_MERGE_SKIP_REASONS = ('paths_outside_records', 'schema_invalid', 'draft', 'hold_label', 'head_moved', 'conflict')   # queue.record_skipped.reason; schema_invalid names the failed RECORD_SHAPE_RULES invariant (an empty file included)
RECORDS_MERGE_PER_PASS = 20                        # oldest-first K per pass; the operator's number
RECORDS_MERGE_HOLD_LABEL = 'hold'                  # a PR with this label is skipped (hold_label)
RECORD_SHAPE_RULES = (   # errata v3 (the turso FDE, measured on t-turso/bug-queue, 300 newest records) + v4: the check passes iff every rule holds; nothing stricter yet
    'the diff against main as a whole ADDS files only, all under records/<one new directory>/ (nothing modified or deleted; nothing outside records/; no .scratch/); every added file is non-empty (evidence may hold binaries: no content check)',
    'that directory contains a file matching TRIAGE*.md',
)
# §5: a maintenance campaign is authored by the seat (campaigns.cards.author) with `source` = MAINTENANCE_CARD_SOURCE
# (campaigns.source carries a CHECK whose words are CARD_SOURCES; 0.2.190 §1 corrects the earlier "free text").
# §6: a heal's green outcome is automatic; its red customer-facing write is never automatic and stays a held, durable deferred
# write (0.2.171 §11, 0.2.175 E19). Making it an internal strand needs the write to be a registry row: a later cut (G1).
# 0.2.215 §1 (E107 / deploy.sh L5): a verify session names the clock step that fired it. `VerifySession.step` (the session record, a
# JSON file under runs/<repo>/sessions/, and its wire twin InFlightSession) is OPTIONAL, a VERIFY_STEP_WORDS word. EVERY in-process
# opener writes it: the clock's steps through the RuleStep marker, the merge-to-main hook battery (readiness_battery), the heal's
# automatic re-drive (heal_redrive) and the auto grounding verify (grounding_verify, a hook with no PIPELINE_STEPS row); the HTTP route
# verify.sessions.start NEVER sets it, whatever the credential (a seat's hand post, CI, a person); the HTTP press
# findings.heal.redrive writes heal_redrive_press at open (0.2.225 §3, E124 (59) v2; it wrote none). Absent = a hand post; old records
# read as absent. `step` is on the INTERNAL session wire only (stripped for external callers, like progress).
VERIFY_STEP_WORDS = ('readiness_battery', 'candidate_verify', 'external_verify', 'served_remeasure',   # the PIPELINE_STEPS rows that dispatch a verify
                     'heal_redrive', 'heal_redrive_press', 'grounding_verify')                        # the other in-process openers: the heal's AUTOMATIC re-drive (the merged-PR hook), the person's press findings.heal.redrive (0.2.225 §3), the auto grounding verify (a hook)
# 0.2.215 §2: the deploy gate (deploy.sh D1, DEPLOY_WINDOW_MARKER) holds iff a session is in flight AND its `step` is absent: a hand post.
# 0.2.225 §3 (E124 (59), corrected v2): the hold also waits for an in-flight session whose step is a DEPLOY_HOLD_STEPS word (typed
# against VERIFY_STEP_WORDS; a person is waiting on these: the person-pressed re-drive); every other opener (heal_redrive, battery,
# candidate, external, remeasure, cut) never holds (each re-fires after the restart: INFRA_REFIRE_BUDGET, 0.2.212's paused outcome). The rule reads ONE field; requested_by
# is never consulted. Both deploy gates (box_jobs_gate, verify_sessions_gate) read this one rule. Ops-side data (generated/ops/deploy.txt),
# not a conductor constant.
DEPLOY_HOLD_STEPS = ('heal_redrive_press',)   # 0.2.225 §3 (E124 (59) v2): VERIFY_STEP_WORDS words a person is waiting on; a hand verify holds through `step absent`
DEPLOY_HOLD_RULE = 'in_flight and (step absent or step ∈ DEPLOY_HOLD_STEPS)'
INFRA_REFIRE_BUDGET = 1
EXTERNAL_VERIFY_WITHIN_HOURS = 24
# 0.2.176 §5b (the operator, 2026-10-25: every agentic job's true cost recorded, whoever pays): credits.db migration 2,
# `agent_costs`, one row per agentic box job (a job with no agent tokens writes none): the payer, what it was for (a
# CREDIT_REASONS word where one applies, else the BOX_JOB_KINDS word) and its ref, the model proxy's token counts, the true
# cost and what the customer paid (0 for an Aretta-paid job); the write appends `job.cost_recorded`. Pricing review is one
# query: credits_charged against usd_micro, by reason.
AGENTIC_PAYERS = ('aretta', 'customer')
# 0.2.176 §5: a maintenance campaign's estimate, sized by the failure: the readiness battery's failing-unit count times a
# per-unit credit constant (both data; the operator rules the constant's value in the release block)
MAINTENANCE_ESTIMATE = ('failing_units', 'minutes_per_unit')   # 0.2.209: minutes, priced like a job of its kind (credits_per_unit and estimated_duration retire)
# 0.2.189 §7 (E66): the maintenance card's estimate fields. estimated_by = MAINTENANCE_ESTIMATED_BY when the rule, not a person,
# sized it; estimated_cost_credits = MAINTENANCE_ESTIMATE in whole credits (the credit_ledger unit); estimated_duration = the
# operator's constant, conductor.toml [maintenance] estimated_duration (no default). The card's status sentence: superseded by
# MAINTENANCE_CARD_WORDS' strand_comment (0.2.190).
MAINTENANCE_ESTIMATED_BY = 'maintenance_estimate'
# 0.2.190 §3 (E68.3): the maintenance card's words are templates; the conductor fills the placeholders and never composes text.
# Every field but `strand_comment` is a campaigns column; `strand_comment` is the seat's FIRST COMMENT ACT on the campaign strand
# the finalize opens (0.2.180), rendered on both fronts (it supersedes 0.2.189's "status sentence" above: status_reason stays
# for discard, withdraw and retire). Emitted to vocabulary.rs.
MAINTENANCE_CARD_WORDS = (   # (campaign field, template); placeholders <repo>, <n> = failing_units, <commit> = to_commit's short form; approved by the operator 2026-10-27 ("approve both")
    ('title',                    'Harness maintenance for <repo> after the upstream move to <commit>'),
    ('objective',                'Bring verification back to green on <repo> at <commit>: <n> failing units.'),
    ('client_voice_description', 'The upstream code moved and <n> of the verification checks no longer run cleanly on it. This campaign repairs them so verification continues on the new code.'),
    ('strand_comment',           'Estimated from <n> failing units of the readiness battery at <commit>.'),
)
COMMIT_SHORT_HEX = 12   # the short commit form everywhere a sentence shows one (the trackalert's existing length)
# 0.2.188 §2: who pays. A job is agentic iff its manifest carries `cost` (routes/box/schemas/JobCost.schema.json); the payer is
# not in the manifest and is not inferred from it: the dispatch names it. A payer is an AGENTIC_PAYERS word or None (a
# command job: no cost, no row); an Aretta-paid row draws nothing (credits_charged 0). A cost the conductor cannot match to a
# dispatch is a DRIFT, pinned once.
JOB_PAYERS = (   # (what dispatched the job, the agent_costs.reason it records, who pays)  — 0.2.176 §3's agentic-cost column, now per dispatch
    ('campaigns.cards.launch (a campaign)',                     'launch',       'customer'),   # the existing draw reason
    ('a conversation turn (0.2.182)',                            'chat_message', 'customer'),
    ('mirrors.track.request_sync (the maintenance campaign)',   'maintenance',  'customer'),
    ('findings.heal.redrive and the heal reconciler',           'heal',         'aretta'),
    ('the readiness battery and the harness fix (0.2.176 §2)',  'pipeline',     'aretta'),
    # 0.2.207 (forge job cost): produced by the conductor's pull of the box's GET /v1/jobs on the campaign observer tick, per org,
    # one agent_costs row per job not yet recorded (job_kind the job's kind; ref owner/repo#N for an issue or a publish, NULL for a
    # memory reflect), from the forge_cost_cursor mark. Rule (g): a forge job whose issue is a campaign's issue_ref or a heal's
    # repair_issue is recorded by THAT dispatch (reason launch or heal), never as agent; the pull skips it. A job whose cost is
    # unreadable gets the agent_cost_unknown once-mark and one journal WARN and is not retried.
    ('the forge dispatcher: an issue, a publish, a memory reflect', 'agent',    'aretta'),
    ('a CI run (box kind ci)',                                   None,           None),   # command jobs; no cost
    ('a verify run (box kind verify)',                           None,           None),
    ('an image bake (box kind image_bake, 0.2.202)',            'image_bake',   'aretta'),   # the customer's first image is Aretta's cost
)
HOLDER_KINDS = ('person', 'machine')   # 0.2.238 §1, §3: exactly PRINCIPALS (org_reader and platform left HOLDERS and the doors)
# The deprecated `requires` (0.2.110; the person half's refusal and routes.json) as a FIXED map from the new word, so a
# reader keyed on `requires` sees today's words for the rest of 0.x (vyasa's §7; the door's Q6). 0.2.151: a PERSON's
# refusal carries this map's word; a MACHINE's carries the word today's conductor sends (MEASURED by cFold on deda6c8b:
# `person`, a capability's CAP_WIRE word, or this map's word; never `Operate` or `Formalize`); `permission` is the new
# word on both (routes/{customer,admin}/openapi.yaml, Refusal.requires). validate holds that every permission a machine holder can be refused for is in CAPS or here.
# 0.2.156: keyed by the new words, with the old words' values (launch_campaign as launch, curate_campaign as
# campaign_author, manage_access as administer); `staff` leaves with its word, so the forge gate (`read`) reads `read`.
REQUIRES_LEGACY = {
    'read': 'read', 'verify': 'cap:verify', 'launch_campaign': 'cap:verify', 'curate_campaign': 'cap:campaign_author',
    'release': 'cap:formalize', 'configure': 'admin', 'steer': 'operator',
    'manage_access': 'admin', 'operate': 'operator',   # 0.2.238 §1: stamp (→ `Stamp`) retires with the permission
}
# CAPABILITY-MODEL §2's catalogue, KEPT (0.2.150) only as the machine half's wire words (`requires: <Cap>` on a
# machine's refusal) and the minted CI token's stored set; the wire spelling where it differs from the name: the
# conductor keeps emitting `AuthorCard` for campaign_author until the major (T2 W6; serde rename + alias).
CAPS = ('read', 'verify', 'steer', 'campaign_author', 'formalize', 'operate', 'stamp')
CAP_WIRE = {'campaign_author': 'AuthorCard'}            # absent = the name in CamelCase
# An OBJECT check a door keeps beside its permission (0.2.150: the rest became permissions or §3's scope rule).
CHECKS_UNIT = ('target_write_door',)
# 0.2.145 (0.6 slice 1): ONE PRINCIPAL PER ACTOR. Every authenticated request is exactly one principal: a `person`
# (subject `github:<login>` always, staff included: staff is an attribute, never a rewrite), a `machine` (a named
# credential: ci, seat:<name>, service:<name>, or the operator token until slice 4), or the answerer, `org_reader`
# (neither a person nor a machine; named here only, its permission is slice 2's). A door row's `principals` is
# `[person]` (no machine passes: `machine: none`) or `[person, machine]` (a machine passes with `machine`'s capability).
PRINCIPALS = ('person', 'machine')   # 0.2.200 §1 (E94 block N): org_reader and platform leave the principals
# 0.2.238 §3 (registry Principal: PERSON, MACHINE): TRANSITIONAL_PRINCIPALS is DELETED (0.2.200 §1's block N+1): no door row lists
# `platform`; the operator token passes as a person (CREDENTIAL_KIND_PRINCIPAL). `callers` stays documentation on every row.
# 0.2.150: `[machine]` joins (the ledger-event writers: persons never stamp).
DOOR_PRINCIPALS = (['person'], ['person', 'machine'], ['machine'])   # 0.2.238 §3: the two `platform` shapes are gone
# 0.2.238 §2 (registry RT__VERIFY_EVENTS_SUBMIT, RT__VERIFY_ORG_EVENTS_SUBMIT): the two submit seams, RETIRED rows (410 route_retired, no
# successor): nobody passed them at LIVE 2312529f; the kinds they admitted are written in process
RETIRED_SUBMIT_SEAMS = ('verify.events.submit', 'verify.org_events.submit')
# 0.2.145: how a doorless row authenticates (a row has `at`, a door, or `auth`, never both): `none` (public),
# `hmac:<table>.<key>` (a signature under the secret file conductor.schema.json names), `sink_grant` (the per-turn
# chat draft grant). 0.2.146: `self` leaves (its two rows, /auth/verify and logout, are doors); validate refuses a word
# no row uses. The chat sink (`sink_grant`) is the one doorless exception to "one check" until slice 4.
AUTH = ('none',)   # 0.2.220 §1: `sink_grant` leaves with campaigns.chat_draft.create (its one row)
AUTH_HMAC = 'hmac:'
# 0.2.234 §7 (operator 2026-11-08): `credential:<kind>`, a doorless row admitted by a presented credential of that CREDENTIAL_KINDS kind, a
# machine's (CREDENTIAL_KIND_PRINCIPAL): the registry's IdentityAdmission (the box's start callback presents the box credential)
AUTH_CREDENTIAL = 'credential:'
BOX_START_ROUTE = 'verify.box_job.start'   # 0.2.234 §7: the registry's box.job.started, the id in the spec's family.noun.verb form
# 0.2.235 §1 (E122.50; registry RT__INTAKE_FORGE_RECEIVE, secret.forge_hook_secret_conductor): the forge's hooks reach the conductor. The row
# renders the registry's IdentityAdmission(FORGE_SIGNATURE) as the two sibling signature rows render theirs (hmac:<table>.<key>, the key naming
# the secret's file: conductor.toml [forge].hook_secret_file, read once at boot); `events` = the registry's demux words, exactly; no caller.
INTAKE_FORGE_ROUTE = 'intake.forge.receive'
INTAKE_FORGE_AUTH = 'forge.hook_secret_file'   # the hmac: key (conductor.schema.json [forge])
INTAKE_FORGE_EVENTS = ('issue.created', 'pull_request.merged', 'pull_request.closed', 'label.publish',
                       'label.verdict.over_strict_dt', 'label.verdict.bug_persists', 'label.verdict.uncertain', 'issue.edited')
# 0.2.148: a door row's optional `unauthenticated` column: what it answers when no credential is presented, instead of
# the refusal order's 401. `login_redirect` = 302 to the sign-in (the forge gate, a forward_auth consumer's door). Only a
# GET `at: sign_in` row may carry it; validate refuses a word no row uses.
UNAUTHENTICATED = ('login_redirect',)
READS = ('path', 'org', 'entity', 'repo', 'none')   # 0.2.192 §1c (E77): `none`, a platform-scope row, which reads no org or repo (validate: none ⇔ scope platform)
MACHINE_REACH = ('path', 'org')   # census A11; `path` is the default and is never written
CALLERS = ('page', 'seat', 'box', 'cli', 'ci',   # 0.2.200 §1: `ops` retires (the ops role is eliminated)
           'slack')   # 0.2.220 §9: a Slack action button (internal channel for an admin-plane row, the bound customer channel for a customer-plane row)
# 0.2.220 §9 (operator 2026-11-02: the cut also names the buttons): the registry's `callers` for these rows, as data; validate holds
# each row's `callers` equal. page = a dashboard button (a dashboard def id); slack = a notify postable with action buttons.
CALLERS_SET = (
    ('campaigns.card.author',     ('page',)),   # 0.2.221 R4: seat leaves
    ('campaigns.card.feedback',   ('page', 'slack', 'seat')),
    ('campaigns.card.withdraw',   ('page', 'seat')),
    ('campaigns.card.repropose',  ('page', 'seat')),
    ('campaigns.card.launch',     ('page', 'slack')),
    ('strands.strand.comment',    ('page', 'slack')), ('strands.strand.accept', ('page', 'slack')), ('strands.strand.reject', ('page', 'slack')),
    ('strands.strand.defer',      ('page', 'slack')), ('strands.strand.end', ('page', 'slack')),
    ('strands.strand.propose',    ('page', 'seat')), ('strands.strand.revise', ('page', 'seat')), ('strands.strand.withdraw', ('page', 'seat')), ('strands.strand.retire', ('page', 'seat')),
    ('findings.finding.disclose', ('page',)), ('findings.finding.retract', ('page',)), ('findings.finding.curate', ('page',)),
    ('findings.triage.verdict',   ('page', 'slack')),
    ('findings.repair.reject',    ('page',)), ('findings.issue.create', ('page',)), ('findings.upstream_pull.publish', ('page',)),
    ('mirrors.track.reset',       ('page', 'slack')),
    ('live.track_state.lock',     ('page',)), ('live.track_state.unlock', ('page',)),   # 0.2.222 §1 (E124 (37) 1): staff buttons on the internal repo page's track card
    ('live.image.accept',         ('seat', 'page', 'slack')),   # 0.2.222 §2: the seat's CLI, and the image strand's accept button (dashboard / Slack) running the target as the presser
    ('findings.bug.dispose',      ('cli',)), ('campaigns.campaigns.retire', ('cli',)),
)
# 0.2.220 §1 (E122.31): a strand message that also causes a facet route (the comment on a card's strand is the card's feedback
# when the presser holds curate_campaign): (strand row, the route it causes)
STRAND_MESSAGE_CAUSES = (('strands.strand.comment', 'campaigns.card.feedback'),)
# 0.2.220 §8.1 (operator E122.8): a route id is never renamed except by this table, for ONE cut: (old id, new id). Consumers read
# it to follow a rename (the dashboard's defs, the CLI, the plugin); it empties at the next cut.
ROUTE_ID_ALIASES = (   # 0.2.223: no rename this cut (0.2.220 §8.1)
)
# 0.2.223 §2 (E124 (40); operator: "we just want to migrate the existing inbox to internal, thats the only change"; E124 (41) O3, O4;
# E124 (44), (45)): THE HOSTS, the plane→host rule as data (this file; no config file). The org hosts are `<slug>.<domain>`; the STAFF
# HOST is a named branch of the conductor's isolate, STAFF_HOST = STAFF_HOST_SLUG + '.' + the conductor's existing `domain` setting
# (state/live/conductor.schema.json `domain`; derived, no new key; E124 (42) 5: never served as a value). PLANE_HOSTS: an admin-plane
# row answers on EVERY host — the org hosts as today, and the staff host ADDS, with the same door words (`both`); every other plane's
# row answers on the org hosts only (no word). A row's `host` column overrides its plane: `staff` = the staff host ONLY (STAFF_HOST_ROWS,
# exactly; validate: each a served admin GET, read at platform, persons only, called by the page); `both` may be written on an
# admin-plane row to say its plane's rule out loud (the inbox, E124 (45)). On the staff host the dashboard page and /assets are
# served with NO org ReqCtx and the health probes (/healthz, the deploy probe) answer as on every host; the org root, /api and
# /:repo app paths are 404 there. Staff sign-in on the staff host takes no org from the Host (STAFF_HOST_SLUG); the session cookie
# is already Domain=.<domain>. Generated: routes.rs STAFF_HOST / STAFF_HOST_ROUTES (the `staff` rows), routes.json `host` (host_of).
HOSTS = ('both', 'staff')                 # the `host` column's words: both = every host (an admin-plane row's rule), staff = the staff host only
PLANE_HOSTS = (('admin', 'both'),)        # the plane→host rule: an admin-plane row answers on every host; absent = the org hosts only
STAFF_HOST_SLUG = 'internal'              # the staff host's slug: the Host's first label, the org slug no org may take
STAFF_HOST = STAFF_HOST_SLUG + '.<domain>'   # internal.<domain>: <domain> is conductor.toml's `domain` (the org hosts' parent domain)
STAFF_HOST_ROWS = ('identity.staff.whoami',)   # the rows marked host: staff — served on the staff host and nowhere else


def host_of(row):
    """0.2.223 §2: where a row answers: its `host` word, else its plane's PLANE_HOSTS word, else `org` (the org hosts only)."""
    return row.get('host') or dict(PLANE_HOSTS).get(row.get('plane')) or 'org'


# 0.2.223 §1 (E124 (38)): the SHARED card shapes, ONE file each under the customer dir, $ref'd from BOTH planes' components (validate), so a
# future cross-org staff page reuses the shape with no conductor change: (schema, the customer body field that carries it, the admin
# body field that carries it, the enum fields and their vocabulary). TrackCard.locked = state == locked.
CARD_SCHEMAS = (
    ('TrackCard', ('RepoView', 'track'), ('EffectiveTrack', 'track_card'), (('state', 'TRACK_STATES'), ('advance', 'TRACK_ADVANCE_WORDS'))),
    ('ImageCard', ('ConfigView', 'images[]'), None, (('kind', 'IMAGE_KINDS'), ('state', 'IMAGE_STATES'), ('failure', 'IMAGE_BAKE_FAILURES'))),
)
METHODS = ('GET', 'POST', 'PUT', 'DELETE', 'PATCH')
# The id's last segment (vyasa REVIEW-T0 §5 rule 1: a closed verb list), then the verbs T0's
# census needed beyond it (named in the T0 spec PR; renamable with the ids).
VERBS = ('list', 'get', 'put', 'create', 'delete', 'start', 'cancel', 'accept', 'sync', 'reset',
         'lock', 'unlock', 'mint', 'revoke', 'retire', 'check', 'launch', 'author', 'finalize',
         'decline', 'stage', 'withdraw', 'report') + (
         'submit', 'match', 'reject', 'complete', 'disclose', 'redrive', 'publish', 'amend',
         'dispose', 'promote', 'hold', 'detect', 'sweep', 'fold', 'explain', 'unpin', 'revert',
         'seed', 'upload', 'receive', 'login', 'redirect') + (
         'file',) + (   # 0.2.154: findings.finding.file, the FILE act (vyasa's draft names the row)
         'propose', 'revise', 'comment', 'defer') + (   # 0.2.168: the strand routes' verbs
         'request_sync',    # 0.2.176 §5: mirrors.track.request_sync (vyasa's draft names the row)
         'end') + (         # 0.2.182 §1: strands.strand.end (specified-unbuilt until 0.7b)
         'curate', 'retract', 'verdict', 'merge', 'repropose', 'feedback') + (   # 0.2.220: the FINAL names the rows (§§1, 3, 5)
         'whoami',)   # 0.2.223 §2: identity.staff.whoami (vyasa's draft names the row; the org whoami is identity.whoami.get)
# The server qualifier `@repo` (rule 2): only the per-repo twins of the org-root setup routes and
# the per-repo whoami.
QUALIFIERS = ('repo',)


def check_ok(word):
    return word in CHECKS_UNIT


# 0.2.114: a row the spec describes and no conductor serves yet. The column `status` (absent = `served`, never written),
# spelled as 0.2.111's `x-status` value. An unbuilt row leaves ROUTES / DOORS for UNBUILT_ROUTES / UNBUILT_DOORS.
ROW_STATUS = ('served', 'specified-unbuilt')
UNBUILT_KEY = 'status'
UNBUILT_VALUE = 'specified-unbuilt'


def is_unbuilt(row):
    """A row the contract names that no conductor serves yet: not mounted, not in ROUTES or DOORS."""
    return row.get(UNBUILT_KEY) == UNBUILT_VALUE

# 0.2.183 §4 (spec conventions): the tenant directory `<root>/tenants/<slug>/` as data, one row per file or directory (a
# directory is a file in the table's sense): (path under the tenant directory, class, writer, durability). It is state.md §2's
# table held as data; STORES is the subset that are databases (validate: every STORES row's file is a TENANT_FILES row of class
# store). The rows carry today's paths (vyasa's drafts/TENANT_FILES.md, verbatim); the three layout renames of DESIGN option 1
# are NOT ruled and are a later cut if the operator takes them (0.2.184 §1a). Emitted to vocabulary.rs as (path, class, durability).
TENANT_FILE_CLASSES = ('config', 'requests', 'facts', 'store', 'derived', 'credential', 'transient')
TENANT_FILE_DURABILITIES = ('mirror+backup', 'backup', 'none')
TENANT_FILES = (   # (path, class, writer, durability)
    ('live/org.toml',                       'config',     'provision; PUT /admin/orgs/{slug}/platform; the accept, lock, built and unpin routes; the installation webhook; the notify PUTs', 'mirror+backup'),
    ('live/configevents.jsonl',             'requests',   'the conductor (the config ledger, one writer)',                'mirror+backup'),
    ('live/.forge-token',                   'credential', 'provision (the box mints it); rotation',                       'none'),
    ('<theme.logo>',                        'config',     'the logo route',                                               'backup'),
    ('config/',                             'derived',    'clone at provision; fetch on read; push on a customer save',   'none'),
    ('ledger/events.jsonl',                 'facts',      'the conductor; the submitters of SUBMITTERS',                 'mirror+backup'),
    ('observations.log',                    'facts',      'the conductor (the observation tier, unchained, rotated)',     'backup'),
    ('observations.inbound.log',            'transient',  'POST /slack/events; the answerer',                            'none'),
    ('notify/state.json',                   'derived',    'the notify tick',                                              'none'),
    ('runs/<repo>/sessions/',               'facts',      'the verify dispatcher',                                        'mirror+backup'),
    ('runs/<repo>/artifacts/',              'facts',      'the verify dispatcher',                                        'mirror+backup'),
    ('identity.db',                         'store',      'the link and unlink routes; the display-name route',           'mirror+backup'),
    ('strands.db',                          'store',      'the strand routes; the system producers',                      'mirror+backup'),
    ('runs.db',                             'store',      'the verify dispatcher; the capability observer',               'mirror+backup'),
    ('findings.db',                         'store',      'the curation routes; the matcher; the report',                 'mirror+backup'),
    ('bugs.db',                             'store',      'the bug routes; the matcher',                                  'mirror+backup'),
    ('credits.db',                          'store',      'the credit routes',                                            'mirror+backup'),
    ('campaigns.db',                        'store',      'the campaign routes',                                          'mirror+backup'),
    ('editions.db',                         'store',      'the promote route; the served-books convergence',              'mirror+backup'),
    ('pulls.db',                            'store',      'the publisher',                                                'mirror+backup'),
    ('clones/<repo>/',                      'derived',    'sync, pull, verify targeting',                                 'none'),
    ('books/fork.git',                      'derived',    'the serve/materialize plane',                                  'none'),
    ('books/main/',                         'derived',    'materialize',                                                  'none'),
    ('books/dev/<arc>/',                    'derived',    'materialize',                                                  'none'),
)
# a store's durability is its SNAPSHOT (VACUUM INTO), never the raw file (state.md §6); `derived/` is not a row: it is the fold's cache
# and retires with fold-retirement release 2 (state.md §12), so the ideal has no such directory.
# 0.2.183 §2 (Rule A): the exported names that fit no form today, (module, old name, new name | None). Three new names are
# ruled (§7.1); None = pending, no name ruled yet. All land in one rename cut after fold-retirement release 2; until then
# validate passes these keys and refuses any other name that fits no form.
VOCABULARY_RENAMES = (
    ('ledger_kinds_vocab', 'KINDS', 'RECORD_KINDS'),
    ('ledger_kinds_vocab', 'SUBMITTERS', 'RECORD_SUBMITTERS'),
    ('postable_vocab', 'POSTABLE', 'POST_RULES'),
    ('codes_vocab', 'RESERVED', None), ('codes_vocab', 'REFUSAL_FAMILY', None),
    ('labels_vocab', 'SET_BY', None), ('labels_vocab', 'READ_BY', None), ('labels_vocab', 'B', None),
    ('labels_vocab', 'Q', None), ('labels_vocab', 'BQ', None), ('labels_vocab', 'RETIRED', None),
    ('labels_vocab', 'SEAT_LOCAL', None),
    ('ledger_kinds_vocab', 'IN_PROCESS_ONLY', None),
    ('postable_vocab', 'INTERNAL_POSTS_0_2_181', None), ('postable_vocab', 'POST_ON_STATE_CHANGE_ONLY', None),
    ('postable_vocab', 'AUDIENCES', None),
    ('registry_vocab', 'AT', None), ('registry_vocab', 'AUTHENTICATED', None), ('registry_vocab', 'PROVENANCE', None),
    ('registry_vocab', 'LEGACY_CLI_PREDICATE', None), ('registry_vocab', 'REVOCATION_FOLD_CHAIN', None),
    ('registry_vocab', 'REVOCATION_ROW_WHEN', None), ('registry_vocab', 'SURFACE_OF_KIND', None),
    ('registry_vocab', 'DISPLAY_NAME', None), ('registry_vocab', 'COMMENTABLE', None),
    ('registry_vocab', 'STORE_VOCABULARY', None), ('registry_vocab', 'BARE_LOGIN_CLOSE', None),
    ('registry_vocab', 'MAINTENANCE_ESTIMATE', None), ('registry_vocab', 'CHECKS_UNIT', None),
    ('registry_vocab', 'AUTH', None), ('registry_vocab', 'UNAUTHENTICATED', None), ('registry_vocab', 'MACHINE_REACH', None),
    ('registry_vocab', 'UNWITNESSED_BY_RULE', None),   # 0.2.233 §1: the FINAL's name; fits no form (a subset of the routes by rule)
)

# 0.2.184 §1 (E43; state.md §6, invariant 12): the DR mirror's triggers, (path, trigger): what makes the mirror worker commit and
# push; a snapshot is VACUUM INTO, never the raw file. Rule: every file that is a WRITE TRUTH, per-tenant or not, has a row; a
# TENANT_FILES row of class `derived` has none (below). A tenant path is a TENANT_FILES path; `<store>.db` is every STORES_LIVE store's file (the store's
# snapshot is retaken after each committed write); `{a,b}` names two paths.
MIRROR_TRIGGERS = (
    ('ledger/events.jsonl',                 'append'),
    ('live/configevents.jsonl',             'append'),
    ('live/org.toml',                       'write'),
    ('runs/<repo>/{sessions,artifacts}',    'write'),
    ('<store>.db',                          'write'),   # for every STORES_LIVE store
    # 0.2.226 §4 (E31 release 2): the global campaigns/campaigns.db row LEAVES: the file is read-only to the head from §4a's copy (its last snapshot stands in the mirror; §4b: it stays in block 14, deleted in a later block); the per-tenant campaigns.db is a hooked write under the `<store>.db` row (0.2.189 §8, E60)
)
# rule: every file that is a WRITE TRUTH, per-tenant or not, has a row; a TENANT_FILES row of class `derived` has none. The
# per-tenant campaigns.db copy is DERIVED in E31 release 1 (re-derived by a plain connection, not a hooked write): it has no
# trigger row of its own and rides any tenant-mirror commit when its bytes differ; release 2 re-engages the `<store>.db` row
# through the hooked write with no data change (0.2.189 §8, E60). A mirror commit whose only difference would be a re-derived
# copy with equal content is no commit.
MIRROR_TRIGGER_EVENTS = ('append', 'write')
# the files outside a tenant directory that are a write truth (the global campaigns file, until E31 release 2): each has a
# MIRROR_TRIGGERS row (0.2.184 §1)
GLOBAL_WRITE_TRUTHS = ()   # 0.2.226 §4 (E31 release 2): none; campaigns/campaigns.db was the one row until this tag
# store-trigger commits are bounded: after a store write the worker retakes that store's snapshot and commits at most once per
# interval per store, with the latest state when the interval ends; append triggers are unaffected; whatever is pending is
# committed at shutdown (0.2.184 §1, cB's commit-rate bound)
MIRROR_STORE_MIN_INTERVAL_SECONDS = 60
# 0.2.184 §2 (E45): the gate kinds of the actions model: a promote reached by a strand accept or a direct call is `press`, the
# pipeline's rule-gated step `rule`
GATE_KINDS = ('rule', 'press')

# 0.2.194 §1 (gitstore fetch hardening): the GitHub App installation's state, org.toml `installation_state` (org.schema's enum is
# exactly these words; ABSENT reads `active`, tolerate-first for every existing org). Written by the conductor on the App's
# installation lifecycle (created / unsuspend -> active, suspend -> suspended, deleted -> deleted), one config-ledger line of kind
# INSTALLATION_LEDGER_KIND per change; never by a route (installation_id_readonly's rule extends to the key). When the state is
# not `active` the SUT sync, pull and webhook paths refuse with app_installation_missing (409) before any token mint or git spawn.
INSTALLATION_STATES = ('active', 'suspended', 'deleted')
INSTALLATION_LEDGER_KIND = 'installation.changed'   # configevents kind, entity `installation`, fields installation_state + installation_id
# 0.2.194 §2/§3: a mirror fetch's refusals before any git spawn: no credential while the installation is active, and the cool-down
GITSTORE_FETCH_CODES = ('mirror_token_unavailable', 'mirror_cooling_down')
# The cool-down: after GITSTORE_FETCH_FAILURES_MAX consecutive failures per (org, repo) the path is refused for
# GITSTORE_FETCH_COOLDOWN_SECONDS; a success resets the count. During the pause the path refuses with mirror_cooling_down (with
# `retry_after_seconds`) without spawning git and logs nothing per request (one WARN when the pause starts). Data, not a config knob.
GITSTORE_FETCH_FAILURES_MAX = 3
GITSTORE_FETCH_COOLDOWN_SECONDS = 300

# 0.2.195 §1 (pending App installations, adopted by id): a pending record's `repository_selection` (as GitHub sends it); the
# record (layout PENDING_INSTALLATION_SCHEMA) lives at PLATFORM_FILES' row under layout PENDING_INSTALLATIONS_DIR, written when an
# installation.created or installation_repositories.added names an id NO org's installation_id holds (a re-delivery rewrites the
# same file), removed on installation.deleted. The rollback floor reads no state/installations/.
INSTALLATION_REPO_SELECTIONS = ('all', 'selected')
# 0.2.195 §4: `conductord provision <org> ... --installation <id>` adopts the pending record (installation_id, its repos,
# installation_state = active, one installation.changed line, the file removed); its refusals write nothing
INSTALLATION_ADOPTION_CODES = ('installation_not_pending', 'installation_already_adopted', 'installation_internal_org')
# 0.2.195 §5b (E85): the box provision row's per-SUT target pair, REQUIRED; the conductor always sends it (an untracked SUT: the
# upstream default-branch head and that branch) or refuses sut_upstream_unresolved before any box call
PROVISION_TARGET_FIELDS = ('track', 'resolved_sha')
# 0.2.196 §3 (E91): the reasons a SUT is left out of a box provision call (0.2.195 §5b), one list for the routes and the provision
# CLI's `omitted[]` report
SUT_OMISSION_REASONS = ('no_position_manual', 'track_unresolved', 'sut_not_synced', 'git_failed')
# 0.2.199 §1 (E98): books-sync's one editable present file, and the kit's GENERIC layer. The edit is the kit's own multibook step
# (tools/multibook-migrate.sh:247-283): exactly two changes, applied together when the fork indexes at least one book, judged on
# parsed data, every other byte preserved: (a) remove EVERY item `atlas` or `atlas/` from `mixed` in one pass (0.2.201 E100: both forms present took two syncs); (b) add each
# FLAVORS_GENERIC_LAYER_PATHS path as the first items of `common.paths` when absent, the inserted line carrying the kit's comment
# verbatim ("# the GENERIC layer (multi-book forks, ops#118)"). Idempotent: a fork already in that state is untouched. The
# existing books-materialized journal line names the file (no new word).
BOOKS_SYNC_EDITABLE_FILES = ('flavors.yaml',)   # 0.2.199 §1 (E98): the only present file books-sync may change; every other present file is left as is
FLAVORS_GENERIC_LAYER_PATHS = ('atlas/**',)       # 0.2.199 §1 (E98): the paths the kit's multibook step gives common.paths, inserted first
# 0.2.197 §2 (cPoll, A6): the pipeline clock's polling bounds. A merged cut/ PR whose edition.cut cannot be recorded is re-read at
# most CUT_RECORD_RETRY_BUDGET ticks (one failed attempt per clock tick per cut; 0.2.199 §3, E97: passes started by wakes do not
# consume the budget), then left until restart (one internal post, cut.unrecorded; the journal WARN names the last
# cause); a wake starts at most one extra pass per CLOCK_WAKE_COALESCE_SECONDS (wakes inside the window fold into one pass at its
# end); the 60 s tick is unchanged.
CUT_RECORD_RETRY_BUDGET = 5
CLOCK_WAKE_COALESCE_SECONDS = 10
# 0.2.197 §3 (CHARTER-box-retire-sweep-cost items 1, 3, 4): the box's job-account retire-sweep bounds, read from its config.toml
# [forge] table (state/box/config.schema.json: these keys, these defaults). The box persists confirmed-retired accounts, the pending
# set and a one-row page cursor in its own index store, so an account done once is never re-called; the sweep lists job accounts
# only. 0.2.206 (the operator's ruling via the door, 2026-10-29; replaces 0.2.197 §3's held deletion): a retired job account is
# DELETED once [forge] job_account_delete_after_days (default 7) have passed since its retirement, within the per-pass call cap,
# oldest retirement first; before the delete call the box writes the pair (job id, forge login) with the retirement and deletion
# times to its own index store; a failed deletion is retried under job_sweep_failure_backoff_seconds like a retire; a running or
# unended job's account and a non-job account are never deleted; one journal INFO line per pass naming the count deleted.
BOX_JOB_SWEEP_DEFAULTS = {'job_sweep_interval_seconds': 300, 'job_sweep_calls_per_pass': 200, 'job_sweep_failure_backoff_seconds': 3600}
# 0.2.198 §1 (E95): in E31 release 1 the global campaigns/campaigns.db is the single write truth; the per-tenant campaigns.db files are
# derived read copies. A migration applies to the write truth and a copy carries the truth's stamp: migration 25 (card_sources) applies
# to the global file (23 -> 25) and every copy is re-derived at 25 (this supersedes E31 release 1's "the global file never migrates
# past 23"). One stamp: the ceiling, the additive check and floor_opens apply to the write truth; a copy is never stamped on its own.
# Three blocks, tolerate first: (1) a derive that tolerates a copy stamped 24 or 25 and readers that hold `maintenance`; (2) the head
# applies m25 to the global and re-derives every copy, m25 marked floor_opens on block 1's measurement; (3) the m24 per-tenant cutover.
CAMPAIGNS_WRITE_TRUTH = 'tenant'    # 0.2.226 §4 (E31 release 2): the per-tenant campaigns.db takes every write (cards, launches, completes, retires, chat conversations, credit_pool when no credits backend); the global file is READ-ONLY to the head from §4a's copy and STAYS in block 14 (§4b: its deletion moves to a later block, once a floor that reads the tenant stores exists; the §4a merge keeps running); `global` was release 1 (0.2.198 §1)
# 0.2.202 §3: the image request's states, one request, in order; built = both kinds pinned for the accepted commit; a failure
# state names ONE failure code per kind. dispatched (an image.dispatched line), baking (the box started the job) and the request-
# level failures are derived by the conductor from the job's state and the report; the dashboard's image step shows the state
# and, on a failure, the code's sentence (WireLabels' pending_reason_<failure>); its words are the dashboard's.
IMAGE_REQUEST_STATES = ('requested', 'accepted', 'dispatched', 'baking', 'built', 'build_failed', 'probe_failed')
# 0.2.202 §3b: the config-ledger kind image.dispatched (entity image, written by the conductor, one line per submit): its fields.
# attempt = the count of image.dispatched lines for this commit, this one included. A dispatch is OPEN while any of its kinds has
# no image.built line after it; the boot pass re-polls open dispatches; an accept re-dispatches only when no dispatch is open
# and a kind is owed; a job that ends with no report closes its dispatch with image.built state = build_failed, failure = other
# per owed kind. The PR ships with image_bake_dispatch = false and becomes the floor before the flip.
IMAGE_DISPATCHED_FIELDS = ('commit', 'job_id', 'attempt', 'image_kinds')
# 0.2.202 §4: the probe's sample books tree is catalogue data, the catalogue repository's `probe-tree/` at the plan's
# catalogue_commit (sha-pinned; reproducible); no test fixture path, no org checkout ([verify] image_bake_probe_tree = catalogue).
# 0.2.202 §5 (measured by ops 2026-10-29): a baked image's label naming the catalogue commit (40-hex) it was baked at; the live
# base images already carry it (95eb07a7). The bake writes it on every image; the box resolves "recipe R at catalogue C" to the
# local tag carrying this label with value C.
IMAGE_CATALOGUE_LABEL = 'org.opencontainers.image.revision'
# 0.2.202 §6: no new token class. The job does not call live.image.report: the conductor reads the job's completion (the
# verify run's path) and appends image.built itself, moving the pin on state = built; the completion manifest carries the TYPED
# field image_bake: {kinds: [{kind, report: BuiltReport}]} (never parsed text); the submit is idempotent on (org,
# request_commit, attempt). live.image.report stays for the hand fallback, called by the seat; a job that posts it is not
# admitted (the job holds no credential).
# 0.2.207 (forge job cost): the box's GET /v1/jobs page bound, its settle window, and the states it lists (the box's spelling)
BOX_JOB_LIST_LIMIT = 100
BOX_JOB_LIST_SETTLE_SECONDS = 60     # a job lists only once its order key is this old
BOX_JOB_TERMINAL_STATES = ('done', 'done-no-output', 'failed', 'cancelled', 'parked')
# 0.2.216 §1 (the 2026-11-02 jobs-host disk incident: the wrapper full-cloned every Tooling repo into every agent job; turso/bug-queue is a
# 1.1 GB pack): a JobSpec repos[] row's optional `clone` (routes/box/openapi.yaml). The BOX dispatcher writes it PER REPO NAME within the
# tenant, regardless of role (bug-queue is the primary of the bug-funnel jobs and gets the filter there too); every other repo, primary or
# not, clones full; a JobSpec submitted with `clone` is honoured as written. INVARIANT (turso FDE): clone -> commit one record on a new
# branch task/issue-<N>-<job> -> push MUST work in the chosen mode. No depth: --depth 1 breaks the resume pin to an older base_sha (rc 128).
BOX_CLONE_FILTERS = ('blob:none',)                                   # the `clone.filter` words (the openapi enum): a partial clone
BOX_CLONE_DEFAULTS = (('bug-queue', {'filter': 'blob:none'}),)      # (repo name within the tenant, the `clone` object the dispatcher writes)
# applied by the wrapper in the SAME `git clone` call on EVERY clone, with or without `clone`, as --config key=value (persisted into the
# clone's config; `git -c` does not persist, measured): no repack inside a job, ever, including a resume's fetch. Tolerate-first: an older
# wrapper ignores the unknown `clone` key (full clone); an older dispatcher writes none; neither side refuses the other.
BOX_CLONE_CONFIGS = (('gc.auto', '0'), ('maintenance.auto', 'false'))
# 0.2.217 §1 (the turso FDE's bug-queue profile: ~90 % of the 1 GB pack is two task branches that committed a toolchain and target/ under
# .scratch/): a push to the tenant's queue repo (bug-queue) on a QUEUE_REPO_GUARDED_BRANCH_PREFIXES branch is REFUSED when any path the new
# commits add or change lies outside QUEUE_REPO_RECORD_PATHS. Enforced HARD at the forge (a pre-receive hook on the queue repo, diff-tree
# per ref update; Forgejo ships DISABLE_GIT_HOOKS=true, so enabling it is the operator's keyboard, not a product row) and EARLY at the
# box (the wrapper's own push path runs the same diff-tree check before pushing and refuses with the word; a pre-push hook seeded at
# materialize is a courtesy, --no-verify bypasses it, the wrapper's check cannot be bypassed). Not the box forge gate (it streams the
# pack and sees refs only). The job sees the refusal (a non-zero push, the word in the output); the job is never cancelled for it.
QUEUE_REPO_RECORD_PATHS = ('records/',)                     # the only path prefix a job's commit to the queue repo may touch
QUEUE_REPO_GUARDED_BRANCH_PREFIXES = ('task/',)             # the branches the guard reads (a job's task branch)
QUEUE_REPO_PUSH_REFUSAL = 'queue_push_outside_records'      # the refusal word the job sees; payload: the offending paths (first QUEUE_REPO_PUSH_REFUSAL_PATHS_MAX), the branch
QUEUE_REPO_PUSH_REFUSAL_PATHS_MAX = 10
# one observation per refusal, observation tier, the box's: (kind, its fields); not a conductor ledger KINDS row (no box submitter)
QUEUE_REPO_PUSH_OBSERVATION_KIND = 'queue.push_refused'
QUEUE_REPO_PUSH_OBSERVATION_FIELDS = ('tenant', 'repo', 'branch', 'job', 'paths')
# 0.2.209 (pricing and job time): the model. rate = org.toml credit_rate_per_hour (> 0, 0.2.208; absent reads org.default.toml's
# CREDIT_RATE_DEFAULT); estimate = the card's estimated_minutes (>= 1; an Aretta-paid job: the launcher's estimate or the kind's
# max); charge = credits_estimated = estimate x rate / 60 rounded up, drawn at launch, fixed price (never topped up or refunded);
# true cost = minutes_true and credits_true recorded at completion in credits.agent_costs (never drawn); alarm limit = min(estimate,
# the kind's max for the tenant), the box ALARMS; hard ceiling = one per-tenant limit for all jobs, the box KILLS; a kind max above
# the ceiling is clipped to it; a `ci` job stays KILLED at its kind max. The rate caps nothing (the credits / rate wall retires).
# A card with NULL estimated_minutes is grandfathered (its stored cost is its charge; its limit is the kind's max). Chat is priced
# per token from conductor.toml [chat] tokens_per_credit; maintenance like a job of its kind (failing_units x [maintenance]
# minutes_per_unit; no card when failing_units is 0). The box honours a forge issue's typed `aretta.estimated_minutes` only from
# an author in its [forge] bridge_logins, min'd with the kind max; `[control] default_wall_seconds` is retired (read and ignored).
JOB_MAX_MINUTES = (('ci', 60), ('agent', 480), ('verify', 60), ('conversation', 60), ('image_bake', 120))   # per kind, the platform default; a tenant overrides per kind in org.toml [limits] job_max_minutes
JOB_HARD_CEILING_DEFAULT_MINUTES = 480   # every job, every kind; per tenant ([limits] job_hard_ceiling_minutes); kill
JOB_HARD_CEILING_MAX_MINUTES = 1440      # the bound on a tenant's ceiling (0.2.209 §2b); the conductor refuses above it (job_limit_invalid)
CREDIT_RATE_DEFAULT = 10                 # org.default.toml credit_rate_per_hour (the code constant retires)
TOKENS_PER_CREDIT_DEFAULT = 1000         # conductor.toml [chat] tokens_per_credit (the code constant retires)
PRICING_LEDGER_KIND = 'pricing.changed'  # configevents kind, entity `org`, fields key, old, new, by; written by pricing.rate.put and pricing.limits.put
PRICING_CHANGED_FIELDS = ('key', 'old', 'new', 'by')
# 0.2.212 §1 (row 8): the per-repo clock pause. Every PIPELINE_STEPS dispatch reads the org's [limits] clock_paused before firing; a step
# whose repo is listed does not fire (one journal INFO pipeline.paused per skipped fire); the pause is per repo, never per org as a whole.
CLOCK_LEDGER_KIND = 'clock.paused'   # configevents kind, entity `org`, fields repos (the whole list as written) + by; written on EVERY call, an unchanged list too
CLOCK_PAUSED_FIELDS = ('repos', 'by')
