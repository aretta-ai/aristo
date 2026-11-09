"""The enum schema files under routes/<plane>/generated/ (0.2.121): one row per file, from the vocabularies.

scripts/generate-routes.py writes each file and lists every path in generated/manifest.json (`generated_schemas`);
scripts/validate.py compares each file and the manifest against these same rows. The hand-written openapi.yaml of the
plane references each file by one `$ref` line, `<Name>: { $ref: "./generated/<Name>.json" }`. The file is named
<Name>.json, the component's own name: openapi-typescript then renders the component in place, so types/ is the same
as when the enum was written inline (a <Name>.schema.json file renders as a `"<Name>.schema"` alias instead).
"""
import json

import codes_vocab as C
import layout as L
import postable_vocab as P
import registry_vocab as R

ID = L.ENUM_SCHEMA_ID
PAGE_CODES = ('every refusal code a conductor answer may carry, then the reserved names (scripts/codes_vocab.py); '
              'the page maps a code to its sentence')
FAMILY = 'which plane answered (scripts/codes_vocab.py REFUSAL_FAMILY; not the registry\'s families)'
BLOCKED = 'the seven reasons a sink blocks a binding (scripts/postable_vocab.py BLOCKED_REASONS)'
PERMISSION = ('the permission vocabulary (0.2.150, 0.6 slice 2; scripts/registry_vocab.py PERMISSIONS): a door row\'s permission, a refusal\'s '
              '`permission`, whoami\'s `permissions`. 0.2.156: the eleven words, then the retired words `launch`, `campaign_author`, '
              '`administer` and `staff` (PERMISSIONS_DEPRECATED), DEPRECATED: kept for 0.x, never served by a 0.2.156 conductor')
SIGN_IN = 'every sign-in code: the vocabulary\'s sign_in layer (scripts/codes_vocab.py), in its order; the page maps it to its signin.* sentence'


def postable(audiences):
    return ('a postable kind of audience ' + ' | '.join(audiences) + ' (scripts/postable_vocab.py); then the retired kinds, read for one release (POSTABLE_RETIRED)',
            [r[0] for r in P.POSTABLE if r[1] in audiences] + [k for k, a in P.POSTABLE_RETIRED if a in audiences])


def rows():
    """(plane, name, description, enum) per file, in the planes' order."""
    out = []
    for plane, audiences in (('customer', ('customer',)), ('admin', P.AUDIENCES)):
        out.append((plane, 'PostableKind') + postable(audiences))
        out.append((plane, 'RefusalCode', PAGE_CODES, C.page_codes()))
        out.append((plane, 'RefusalFamily', FAMILY, list(C.REFUSAL_FAMILY)))
        out.append((plane, 'BlockedReason', BLOCKED, [c for c, _ in P.BLOCKED_REASONS]))   # 0.2.130 (B31)
        out.append((plane, 'Permission', PERMISSION, list(R.PERMISSIONS) + list(R.PERMISSIONS_DEPRECATED)))   # 0.2.150; 0.2.156
    # 0.2.181 §4: the cross-tenant inbox's `kind` parameter admits the internal strand kinds only (STRAND_KIND_AUDIENCE)
    out.append(('admin', 'InternalStrandKind', 'the strand kinds whose audience is internal (scripts/registry_vocab.py STRAND_KIND_AUDIENCE); the cross-tenant inbox filters by one',
                [k for k, a in R.STRAND_KIND_AUDIENCE.items() if a == 'internal']))
    # 0.2.231 §2 (E124 (77c)): the Slack save's kinds: the customer-audience postable rows whose front is slack (the same set as org.schema
    # notify.slack.events and notify.schema.json's events[] since 0.2.230); NotifyBinding.events and the PUT body's events are this type
    out.append(('customer', 'SlackPostableKind', 'the customer-audience postable kinds whose front is slack (scripts/postable_vocab.py): what a Slack binding may carry; the postable\'s front is its surface (0.2.230 §1, 0.2.231 §2)',
                [r[0] for r in P.POSTABLE if r[1] == 'customer' and len(r) > 9 and r[9] == 'slack']))   # the length guard: a malformed row is validate's to refuse, never a generator crash
    # 0.2.131: the sign-in codes, the vocabulary's sign_in layer in its order (the customer plane's SignInRefusal.error)
    out.append(('customer', 'SignInCode', SIGN_IN, [r[0] for r in C.CODES if 'sign_in' in C.layers(r)]))
    # 0.2.215 §1: the clock step (or in-process hook) that opened a verify session; absent on the wire = a hand post
    out.append(('customer', 'VerifyStep', 'the in-process opener that fired a verify session (scripts/registry_vocab.py VERIFY_STEP_WORDS); absent = a hand post through verify.sessions.start',
                list(R.VERIFY_STEP_WORDS)))
    out.append(('box', 'RefusalCode', 'the box\'s coded refusals (scripts/codes_vocab.py, layer box with a status)',
                C.box_refusals()))
    out.append(('box', 'JobDiagnostic', 'the box\'s job-state and prune words (layer box, no status)',
                C.box_diagnostics()))
    return out


def path(plane, name):
    return L.generated_schema(plane, name)


def document(plane, name, description, words):
    return {'$schema': 'https://json-schema.org/draft/2020-12/schema', '$id': ID.format(plane=plane, name=name),
            'type': 'string', 'description': description, 'enum': list(words)}


def text(row):
    return json.dumps(document(*row), indent=2, ensure_ascii=False) + '\n'
