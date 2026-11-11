"""What the notify sink may say (T1). One row per postable kind: the kind the binding's `events[]` names, its audience
(customer | internal), its SOURCE (which conductor store the tick reads and the record kinds that produce it), the entity
the dedupe key names, and the sentence code the dashboard's copy keys the sentence on. Adding or removing an event is one
row here plus one sentence in the dashboard's copy; a kind removed here drops out of every binding's events[] at the next
read. The sentences are the dashboard's (src/copy.ts), never this file's.

0.2.123 (B27): `slots` names the placeholders the renderer fills for a kind. A sentence's `{…}` placeholders are exactly
its kind's `slots`; the check sentence has none (`CHECK_SLOTS`), nor have the `notify.blocked.<reason>` sentences.
`link` is never a slot: the sink appends the org's dashboard link on its own line after the sentence.

0.2.124 (B28): internal-audience kinds post ONLY to conductor.toml's `[[notify.internal]]` channels; an org binding
carries customer-audience kinds only. The `audience` column is unchanged; what changes is where each audience is
delivered.

0.2.133 (interactive replies): the answerer's reply in a mention's thread is NOT a postable kind. A postable sentence is
fixed copy the dashboard words; a reply is generated text, so it has no row here, no sentence code, and is outside the
dashboard's copy lint. What this file holds for it is the closed `reason_code` of a `slack.declined` record
(`DECLINED_REASONS`).

0.2.137 (the door's ruling 2026-10-20 20:11:07Z): `onboarding.step_completed` is posted with the sentence of its STEP,
chosen by the producing record's kind (`STEP_SENTENCES`), never with raw ledger words. The kind, its records and its
dedupe entity are unchanged. The row's own `sentence_code` `notify.onboarding_step_completed` and its `{step}` slot are
KEPT but UNPOSTED by a 0.2.137 conductor: a rollback binary still posts it and still finds its sentence. The clock
(0.2.143, the 0.2.111 x-removed form): `notify.onboarding_step_completed` and its `{step}` slot leave in the first cut
after conductor #597's binary (0.2.137) is the rollback floor; until then the dashboard keeps the sentence."""
AUDIENCES = ('customer', 'internal')
SOURCES = ('ledger', 'config_ledger', 'healthz', 'webhook', 'clock', 'box_health', 'notify_state', 'store_marks')   # 0.2.226 §6a: notify_state = the notify tick's state.json standing block (E122.74); store_marks = a store's marks table (E124 (64)); neither has a ledger line (the sink's notify.blocked is observation tier, 0.2.133)   # 0.2.195 §2: webhook = a post from an App webhook delivery with no ledger line; 0.2.197 §2: clock = the pipeline clock's own event, no ledger line          # campaigns.db joins when it stores transitions (B19)

# (kind, audience, source, producing records, entity, sentence_code, slots, buttons, in_place, front, suppress, link)
# 0.2.211 §1: `suppress` is explicit on every row (() = none) and `link`, the twelfth column, is a LINK_PAGES word: the page the
# post's appended link opens (`by_strand_kind`: the strand's TOPIC via STRAND_LINK_PAGES; `by_step`: the step word via STEP_LINK_PAGES).
# 0.2.172 (the Slack front for strands, §5; the build ruling §15.2): three columns join every row: `buttons` ('' or
# 'kind_actions': the strand kind's customer actions other than `comment`), `in_place` ('' or the postable kind whose post
# this row EDITS: its root post's id is read from strands.db `fronts`), `front` (a registry_vocab FRONTS word: where it
# posts; a later `linear` front is one FRONTS value and one delivery function). `suppress` is an optional eleventh column
# (on finding.disclosed only): an origin whose act posts no second notice (a release that ran from a strand accept: the
# edited strand post is the notice).
POSTABLE = (
  # 0.2.224 §6a (E124 (52), operator 2026-11-04; corrected by measurement: disclosure.recorded ALREADY triggers it, no data change): finding_disclosed
  # posts the FIRST time the finding is external — on disclosure.recorded when it is external then, else on the edition.advanced (served) that
  # makes it external (its anchor lands in the served edition); exactly once per finding. The conductor slice: re-judge late-external findings on promote.
  ('finding.disclosed',          'customer', 'ledger',        ('disclosure.recorded', 'finding.issue_linked'), 'finding_id',       'notify.finding_disclosed',         ('finding_id',), '', '', 'slack', ('origin:strand',), 'finding'),
  ('verify.run_failed',          'customer', 'ledger',        ('run.stamped',),                                 'run_id',           'notify.verify_run_failed',         ('run_id',), '', '', 'slack', (), 'run'),
  # 0.2.202 §3c (walk 2 finding 1-5): the image step posts only when an image.built line with state = built exists for BOTH
  # kinds of the accepted commit; a build_failed or probe_failed line posts nothing to the customer (it moves the request state)
  ('onboarding.step_completed',  'customer', 'config_ledger', ('track.accepted', 'image.built', 'onboarding.completed'), 'step',   'notify.onboarding_step_completed', ('step',), '', '', 'slack', (), 'by_step'),
  ('healthz.degraded',           'internal', 'healthz',       (),                                               'org+reason_code',  'notify.healthz_degraded',          ('org', 'reason_code'), '', '', 'slack', (), 'org'),
  # 0.2.219 §3: the jobs host's disk watermark, from the conductor's poll of the box's GET /v1/health (source box_health; the post IS
  # the record: no ledger line, no store). One post per crossing and one per clear; the episode `<host>@<at>` and its delivery state live
  # in the notify channel's state.json. `watermark` = the configured percent (health disk.watermark_percent); `host` = the box's host from
  # [verify] substrate_url. DRAFT sentences (the dashboard's copy, operator approval pending): notify.box_disk_low "Jobs host {host}: disk
  # {used_percent} % used ({free_bytes} free), above the {watermark} % watermark; new jobs wait."; notify.box_disk_cleared "Jobs host
  # {host}: disk back to {used_percent} % used; jobs start again." The two post only once the dashboard copy pin carries them
  # (the cut.unrecorded pattern until then). No org: link `none`.
  ('box.disk_low',               'internal', 'box_health',    (),                                               'host+at',          'notify.box_disk_low',              ('host', 'used_percent', 'free_bytes', 'watermark', 'at'), '', '', 'slack', (), 'none'),
  ('box.disk_cleared',           'internal', 'box_health',    (),                                               'host+at',          'notify.box_disk_cleared',          ('host', 'used_percent', 'free_bytes', 'watermark', 'at'), '', '', 'slack', (), 'none'),
  # 0.2.172 §5: the strand's posts in the org's one bound channel (§15.1: `link` is never a slot; the sink appends it)
  ('strand.opened',  'customer', 'ledger', ('strand.proposed',),  'strand_id', 'notify.strand_opened',  ('kind', 'repo', 'topic', 'version', 'text'), 'kind_actions', '',              'slack', (), 'by_strand_kind'),
  ('strand.updated', 'customer', 'ledger', ('strand.accepted', 'strand.rejected', 'strand.deferred', 'strand.act_failed', 'strand.withdrawn', 'strand.invalidated', 'strand.revalidated', 'strand.retired', 'strand.commented'), 'strand_id', 'notify.strand_updated', ('state', 'actor', 'result'), '', 'strand.opened', 'slack', (), 'by_strand_kind'),
  ('strand.revised', 'customer', 'ledger', ('strand.revised',),   'strand_id', 'notify.strand_revised', ('version', 'text'), 'kind_actions', 'strand.opened', 'slack', (), 'by_strand_kind'),
  # 0.2.181 §5.1: the internal strand posts, to conductor.toml [[notify.internal]] channels only (0.2.124 B28): same source,
  # records and sentence codes as the customer rows, the customer rows' slots PLUS `org`, `repo` and `ref` (an edition topic's
  # token, a track topic's commit; empty otherwise); the row is chosen by the strand's audience. A row name is unique (it keys
  # the generated table), so the internal twin is `<customer row>.internal`. 0.2.185: the twin posts its OWN code,
  # `<customer code>_internal`, whose slots are org, repo, ref and then the customer row's (one sentence per code; the
  # customer codes keep their approved slots).
  ('strand.opened.internal',  'internal', 'ledger', ('strand.proposed',),  'strand_id', 'notify.strand_opened_internal',  ('org', 'repo', 'ref', 'kind', 'topic', 'version', 'text'), 'kind_actions', '',                       'slack', (), 'by_strand_kind'),
  ('strand.updated.internal', 'internal', 'ledger', ('strand.accepted', 'strand.rejected', 'strand.deferred', 'strand.act_failed', 'strand.withdrawn', 'strand.invalidated', 'strand.revalidated', 'strand.retired', 'strand.commented'), 'strand_id', 'notify.strand_updated_internal', ('org', 'repo', 'ref', 'state', 'actor', 'result'), '', 'strand.opened.internal', 'slack', (), 'by_strand_kind'),
  # 0.2.185 §1: an internal strand's revision posts, as a customer one does (notify.strand_revised)
  ('strand.revised.internal', 'internal', 'ledger', ('strand.revised',),   'strand_id', 'notify.strand_revised_internal', ('org', 'repo', 'ref', 'version', 'text'), 'kind_actions', 'strand.opened.internal', 'slack', (), 'by_strand_kind'),
  # 0.2.181 §5.2: the automatic promote and the rollback are told in the internal channel; no strand exists for either, so three
  # flat codes (a sentence cannot print a boolean): the row is chosen by the edition.advanced receipt: a rollback is a receipt
  # whose edition has a prior receipt for the book (0.2.178 §2); otherwise promoted, automatic when the receipt's gate was the
  # rule and not a press. One post per receipt.
  ('edition.promoted',           'internal', 'ledger', ('edition.advanced',), 'receipt', 'notify.edition_promoted',           ('org', 'repo', 'edition', 'previous'), '', '', 'slack', (), 'repo_overview'),   # a pressed promote
  ('edition.promoted.automatic', 'internal', 'ledger', ('edition.advanced',), 'receipt', 'notify.edition_promoted_automatic', ('org', 'repo', 'edition', 'previous'), '', '', 'slack', (), 'repo_overview'),   # a rule-gated promote (customer_visible_change false)
  ('edition.rolled_back',        'internal', 'ledger', ('edition.advanced',), 'receipt', 'notify.edition_rolled_back',        ('org', 'repo', 'edition', 'previous'), '', '', 'slack', (), 'repo_overview'),
  # 0.2.189 §5 (E55): the clock ALARMS when an external run regresses against SERVED_RUN; release 1's rollback is a press (the FDE
  # reverts SERVED on the fork and promotes). Source: the external run's run.stamped compared against SERVED_RUN; one post per
  # regressed receipt; internal words (0.2.185's internal-post rule).
  ('edition.regressed',          'internal', 'ledger', ('run.stamped',),      'receipt', 'notify.edition_regressed',          ('org', 'repo', 'edition', 'previous', 'failing'), '', '', 'slack', (), 'repo_overview'),
  # 0.2.194 §4: the GitHub App installation's state changed (registry_vocab INSTALLATION_STATES); source the config ledger's
  # installation.changed line; posted on every state change, internal channel only. Its sentence_code is EMPTY: the sentence is
  # chosen by the line's `installation_state` (INSTALLATION_STATE_SENTENCES); the words are the dashboard's.
  ('installation.changed',       'internal', 'config_ledger', ('installation.changed',), 'installation', '', ('org',), '', '', 'slack', (), 'none'),
  # 0.2.195 §2: an App installation no org holds was recorded pending (layout PENDING_INSTALLATION_SCHEMA); posted once per new
  # record, never on a re-delivery; `repo_count` is the one field the record does not hold. Internal words (the dashboard's).
  ('installation.pending',       'internal', 'webhook', (), 'installation_id', 'notify.installation_pending', ('installation_id', 'account_login', 'repo_count'), '', '', 'slack', (), 'none'),
  # 0.2.221 §1 (E122.52, E124 (4)/(18)): the automatic cut's notice on the org's INTERNAL channel, no button; keyed once per
  # edition_id (the PR is a slot, not the key). books.cut_green / books_cut_merged and cut.unrecorded RETIRE (the merge records).
  # 0.2.221 E124 (25) O6: the cut dispatch could not fire for a repo; keyed once per org+reason (re-posted only when the reason changes)
  ('cut.dispatch_skipped',       'internal', 'clock', (), 'repo+reason', 'notify.cut_dispatch_skipped', ('org', 'repo', 'reason'), '', '', 'slack', (), 'repo_overview'),
  ('edition.cut_landed',         'internal', 'ledger', ('edition.cut',), 'edition_id', 'notify.edition_cut_landed', ('org', 'repo', 'edition_id', 'pr', 'sha'), '', '', 'slack', (), 'repo_overview'),
  ('triage.filed',               'internal', 'ledger', ('triage.filed',), 'finding_id', 'notify.triage_filed', ('org', 'repo', 'finding_id', 'issue'), 'verdicts', '', 'slack', (), 'finding'),
  # 0.2.226 §6a, §7 (E122.74, E124 (71); operator 2026-11-04): a STANDING delivery block (the notify tick's state.json block; the sink's
  # notify.blocked record is observation tier, so the row reads the STATE, source notify_state, no record) is SHOWN to staff in the
  # internal channel (slots org, channel, reason, since; keyed once per org while the block stands) and to the tenant on its dashboard
  # (the `.dashboard` row, front dashboard: the blocked channel cannot carry its own notice; no `org` on a customer row). A notice, no
  # new state. The words are the dashboard's (notify.blocked.<reason> already words the reason).
  ('notify.delivery_blocked',           'internal', 'notify_state', (), 'org',     'notify.delivery_blocked',           ('org', 'channel', 'reason', 'since'), '', '', 'slack',     (), 'org_setup'),
  ('notify.delivery_blocked.dashboard', 'customer', 'notify_state', (), 'channel', 'notify.delivery_blocked_dashboard', ('channel', 'reason', 'since'),        '', '', 'dashboard', (), 'org_setup'),
  # 0.2.226 §3a, §6a (E124 (63), (64)): a store whose backfill is HELD (marks.state held, reason BACKFILL_HOLD_REASONS) is told to staff
  # once per (org, store) while the hold stands; the org keeps the fold until a person clears it. Source: the store's marks table.
  ('notify.backfill_held',              'internal', 'store_marks',  (), 'org+store', 'notify.backfill_held',            ('org', 'store', 'reason', 'since'), '', '', 'slack',     (), 'none'),
)
# 0.2.221 §1 (tolerate-first): the postable kinds that leave POSTABLE: (kind, audience). Never posted again; a binding or a
# wire enum may still name one for a release (the enum files keep them after the live kinds).
POSTABLE_RETIRED = (('cut.unrecorded', 'internal'), ('books.cut_green', 'internal'))
# 0.2.184 §2 (E45): the rows chosen by a field of their record, never by the actor (a seat may run a ceremony act by hand):
# (postable kind, the record's payload field, its value). The two promote posts select on edition.advanced's `gate`.
POST_SELECTORS = (
    ('edition.promoted',           'gate', 'press'),
    ('edition.promoted.automatic', 'gate', 'rule'),
)
# 0.2.194 §4: installation.changed posts one sentence PER STATE, chosen by its line's `installation_state`; the keys are
# registry_vocab INSTALLATION_STATES and each code's slots are the row's. The words are the dashboard's (approved 2026-10-28).
INSTALLATION_STATE_SENTENCES = {'active': 'notify.installation_active', 'suspended': 'notify.installation_suspended',
                                'deleted': 'notify.installation_deleted'}
# 0.2.181 §5: the internal rows of this cut (validate: audience internal; no customer-audience row carries `org`)
INTERNAL_POSTS_0_2_181 = ('strand.opened.internal', 'strand.updated.internal', 'edition.promoted', 'edition.promoted.automatic', 'edition.rolled_back')
# 0.2.172 §5/§6 (§15.2): the strand front's sentences that are no postable kind. The first-run explanation, one per strand
# kind (deduped by `notify.delivered` entity `firstrun:<kind>`), is GENERATED from registry_vocab STRAND_KINDS
# (strand_firstrun_sentences()); the stub reply answering every comment until 0.7b's agent, and the unlinked presser's
# private link prompt, are named here. None fills a placeholder. The words are the dashboard's, approved there (src/copy.ts rows
# D1–D5, AA3, AA4, 2026-10-25/26). The spec marks DRAFT only on text it holds; a code's served sentence is the dashboard's, and its
# approval is recorded there (0.2.192 §1b).
STRAND_COMMENT_STUB_SENTENCE = 'notify.strand_comment_stub'
# 0.2.180 §2, unit corrected by 0.2.191 §2 (E71): a `strand.commented` record posts only when the strand's state after the act
# differs from the state before; every other `strand.*` update posts (edits the root), a same-state edit included (a retried
# accept that fails again shows the new error). The words are record KINDS rows, each in the strand.updated row's records.
POST_ON_STATE_CHANGE_ONLY = ('strand.commented',)   # record KINDS (not POSTABLE rows) whose post is skipped when the strand's state did not change; every other strand.* update posts
# 0.2.182 §5: the strand kinds whose `strand.opened` is never posted (the customer opened it): POSTABLE's strand rows exclude
# them, so neither their first-run explanation nor their `kind` slot word (both said only beside an opened post) is a sentence.
UNPOSTED_STRAND_KINDS = ('conversation',)
SLACK_UNLINKED_SENTENCE = 'notify.slack_unlinked'
# 0.2.220 §5 (E113): the INTERNAL unmapped press (0.2.211 §2) is refused, not remembered: its own answer (link, then press again),
# never the customer's "saved" sentence. (E118): a press the clock pause holds (reason `paused`) is refused with its own internal
# sentence (lift the pause, then press again), never the generic action_failed. Both staff-only; the words are the dashboard's.
INTERNAL_UNMAPPED_SENTENCE = 'notify.internal_unmapped'
# 0.2.220 §9: a post's buttons: '' none; 'kind_actions' the strand kind's customer actions other than comment; a SLACK_BUTTON_ROUTES
# word, the one route its button presses
BUTTONS = ('', 'kind_actions', 'verdicts')   # 0.2.221: `merge` leaves with books.cut.merge
SLACK_BUTTON_ROUTES = (('verdicts', 'findings.triage.verdict'),)
# 0.2.220 §10.6, §10.7: the DRAFT sentences this cut holds (operator's copy approval pending; the dashboard's copy is the served text)
DRAFT_SENTENCES = (
    ('notify.internal_unlinked', 'Link your Slack account to Aretta first, then press again.'),   # 0.2.221 E124 (18, G6): sentence AL1b approved 2026-11-03
    ('notify.internal_unmapped', "This channel is not bound to a repo, so nothing was saved. Bind it from the dashboard's Slack page."),
    ('notify.press_paused', "The clock is paused for {repo}; this step did not run. Unpause it from the dashboard's repo page."),
    # 0.2.226 §7 (E122.72, E122.73; vyasa 2026-11-05): the check's "add the Aretta app from your workspace" sentence and the picker label
    # "Please invite the @Aretta App to this channel." (the operator's exact words, DECISIONS 2026-11-05T07:22Z) are DASHBOARD ui.* words,
    # not notify sentences: no row here (the conductor's slots::copy_problems requires every SENTENCE_SLOTS code in COPY.notify); the
    # dashboard owns both strings (E122.73).
)
# 0.2.221 E124 (18, G6): an UNMAPPED STAFF USER pressing on a bound internal org gets its own word, with the Link button
# (link, then press again); notify.internal_unmapped keeps its meaning, an UNBOUND CHANNEL (no Link button)
INTERNAL_UNLINKED_SENTENCE = 'notify.internal_unlinked'
# 0.2.220 (dashboard-team 2026-11-02): the slots of the sentence codes that are no postable row: (code, slots)
SENTENCE_SLOTS = (('notify.internal_unmapped', ()), ('notify.press_paused', ('repo',)), ('notify.internal_unlinked', ()),
                  )   # 0.2.226 §7: the check's not-in-channel sentence and the picker label are the dashboard's ui.* words, not rows here
PRESS_PAUSED_SENTENCE = 'notify.press_paused'


# 0.2.174 §9 (dashboard-team's E16, by the door's order): what the strand posts' slots carry. `topic` (renamed from `subject`,
# 0.2.173's one name) is the strand's topic; `kind` the raw STRAND_KINDS value, worded by `notify.strand_kind.<kind>` (every
# kind not in UNPOSTED_STRAND_KINDS has words; 0.2.189 §9, E59); `state`
# a STRAND_STATES value, worded by `notify.strand_state.<state>`; `actor` a principal's subject (a person by the rendering
# rule, any non-person principal by `notify.actor_aretta`); `result` data, never text (the act's link on `released`, the
# target's refusal code on `act_failed`). The link the sink appends is labelled `notify.open_link`. The two groups are
# GENERATED from the vocabulary (no hand list), one nesting level as `notify.blocked.<reason>`; the words are the dashboard's.
ACTOR_ARETTA_SENTENCE = 'notify.actor_aretta'
OPEN_LINK_SENTENCE = 'notify.open_link'


def strand_kind_sentences():
    """0.2.174 §9: `notify.strand_kind.<kind>`, one per registry_vocab STRAND_KINDS value, in its order (0.2.182: a posted kind)."""
    import registry_vocab as _RV
    return tuple(f'notify.strand_kind.{k}' for k in _RV.STRAND_KINDS if k not in UNPOSTED_STRAND_KINDS)


def strand_state_sentences():
    """0.2.174 §9: `notify.strand_state.<state>`, one per registry_vocab STRAND_STATES value, in its order."""
    import registry_vocab as _RV
    return tuple(f'notify.strand_state.{s}' for s in _RV.STRAND_STATES)


def strand_firstrun_sentences():
    """0.2.172: `notify.strand_firstrun_<kind>`, one per registry_vocab STRAND_KINDS word, in its order (0.2.182: a posted kind)."""
    import registry_vocab as _RV
    return tuple(f'notify.strand_firstrun_{k}' for k in _RV.STRAND_KINDS if k not in UNPOSTED_STRAND_KINDS)
CHECK_SENTENCE = 'notify.check'
CHECK_SLOTS = ()          # the check sentence has no placeholder
# `link` is never a slot: the sink appends the org's dashboard link on its own line after the sentence.

# The Blocked record's `reason_code` (state/ledger/README.md §08): the sink's terminal 4xx classes and the Slack
# lifecycle events PR-C maps (product-onboarding 2026-10-17 14:5xZ, confirmed 15:0xZ). One list: the conductor's
# classifier and the dashboard's sentence keys generate from it.
BLOCKED_REASONS = (
  # (reason_code, meaning)
  ('channel_not_found', 'Slack answered channel_not_found: the bound channel no longer exists'),
  ('not_in_channel',    'Slack answered not_in_channel: the bot is not a member of the bound channel'),
  ('account_inactive',  'Slack answered account_inactive: the installing account is deactivated'),
  ('uninstalled',       'the workspace uninstalled the app (app_uninstalled)'),
  ('revoked',           'the token no longer works: tokens_revoked, or invalid_auth at post time; the app may still be installed'),
  ('channel_gone',      'the channel was deleted or archived (channel_deleted | channel_archive)'),
  ('removed',           'the bot was removed from the channel (member_left_channel with the bot as the leaver)'),
)

# 0.2.137: the step -> sentence map of `onboarding.step_completed`, keyed by the producing record's kind; closed at the
# row's three records. `slots` are the placeholders its sentence fills: `repo` is the entity `repos/<repo>` without its
# prefix; `image_kind` is the image.built record's `image_kind` said as its word in `IMAGE_KIND_WORDS` (one sentence per
# image kind: a bake records one Built per kind); the setup step fills none.
STEP_KIND = 'onboarding.step_completed'
STEP_SENTENCES = (
  # (record kind, sentence_code, slots)
  ('track.accepted',       'notify.step.track_accepted',       ('repo',)),
  ('image.built',          'notify.step.image_built',          ('image_kind',)),   # a Built report, as the row posts today
  ('onboarding.completed', 'notify.step.onboarding_completed', ()),
)

# 0.2.137 (the door's ruling (i)): the `{image_kind}` slot's word, a CLOSED map from the image.built record's
# `image_kind` (state/ledger/configevents.schema.json) to the word the sentence says; the raw kind is never posted.
IMAGE_KIND_WORDS = (
  # (image_kind, word)
  ('verify',  'verification'),
  ('sandbox', 'sandbox'),
)

# 0.2.142 (the operator's words 2026-10-21 01:31:14Z + 01:33:15Z): the chat grounding pack groups every category with
# no authored display name under ONE catch-all area. Its customer-facing name is copy the dashboard words, keyed here:
# the key is `chat.area_other` in the dashboard's `COPY.chat` block (key `area_other`); it fills no placeholder. Its
# value is FINAL, "Other findings" (operator 2026-10-21T01:35:22Z "Other findings sounds good"); the dashboard owns the
# sentence and the conductor bakes it at build, as it bakes COPY.notify. It is not a postable sentence: it is the name
# the pack gives the catch-all area, shown wherever the answer is (the dashboard's Ask and a Slack reply).
CATCH_ALL_AREA_CODE = 'chat.area_other'

# 0.2.133: the `reason_code` of a `slack.declined` record (state/ledger/README.md §08, the answerer): why a stored mention
# got no reply. There is no `not_interactive`: while replies are off nothing reads the inbound log, so nothing is recorded.
DECLINED_REASONS = (
  # (reason_code, meaning)
  ('not_bound_channel', 'the mention is in a channel other than the binding\'s channel_id: no reply, its text never read'),
  ('nothing_to_say',    'the answer had nothing the org\'s customer audience may see'),
  ('rate_limited',      'the org reached its hourly reply cap ([slack] interactive_hourly_cap, compiled default 20)'),
  ('model_unavailable', 'the model call failed or did not answer within its deadline'),
  # 0.2.172 §6: a press by a Slack user with no alias on the org: answered privately with the link prompt, nothing acts
  ('unlinked',          'the presser has no alias on the org: a private link prompt; the press is remembered for replay'),
)

# 0.2.211 §1 (operator words 2026-10-29 via the door; LINK-AUDIT 3a5f62d0, LINK-PAGE-IDS 0f649459): each post's link page as data.
# The sink builds the link from the page id, the view and the slot fields; LINK_PAGE_IDS is emitted to the dashboard's generated
# postable.ts so the dashboard's own test holds each id against contract/pages.ts (the spec does not vendor pages.ts). The
# edition rows and cut.unrecorded open repo_overview until the edition page ships (a later cut adds `edition`); the installation
# rows open no page (the admin tool is not a dashboard page).
LINK_PAGES = ('org', 'org_setup', 'repo_overview', 'repo_overview_strand', 'finding', 'run', 'campaigns_strand', 'none', 'by_strand_kind', 'by_step')
LINK_PAGE_IDS = (   # (word, the dashboard page id in web contract/pages.ts, the view within it or ''), measured at web pin 60cb0dae
    ('org',                  'org-root',       'landing'),
    ('org_setup',            'org-root',       'tab:setup'),
    ('repo_overview',        'overview',       ''),
    ('repo_overview_strand', 'overview',       '?strand=<strand_id>'),
    ('finding',              'finding-detail', ''),
    ('run',                  'run-detail',     ''),
    ('campaigns_strand',     'campaigns',      '?strand=<strand_id>'),
)
LINK_PAGE_SLOTS = (   # the fields the sink needs beside the page word (from the row's slots or the record's envelope)
    ('org', ()), ('org_setup', ()), ('repo_overview', ('repo',)), ('repo_overview_strand', ('repo', 'strand_id')),
    ('finding', ('repo', 'finding_id')), ('run', ('repo', 'run_seq')), ('campaigns_strand', ('repo', 'strand_id')), ('none', ()),
)
STRAND_LINK_PAGES = (('finding', 'finding'), ('pull', 'repo_overview_strand'), ('campaign', 'campaigns_strand'),
                     ('edition_promote', 'repo_overview_strand'), ('track_move', 'repo_overview_strand'),
                     ('triage', 'finding'),   # 0.2.221 §2: the triage strand opens its finding
                     ('track', 'repo_overview_strand'), ('image', 'org_setup'))   # 0.2.222 §1, §2: the track strand opens its repo; the image strand the org's setup (the image step)
STEP_WORDS = (('track.accepted', 'track'), ('image.built', 'image'), ('onboarding.completed', 'completed'))   # the step word per STEP_SENTENCES record
STEP_LINK_PAGES   = (('image', 'org_setup'), ('track', 'repo_overview'), ('completed', 'org'))
