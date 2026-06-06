# LingQ API — observed surface

> Status: **observed, not documented**. LingQ does not publish a v3 API reference. Every fact below was verified by probe against the live API and is subject to silent change. Schema-drift detection is handled by the cassette-based contract test strategy in AD-013.
>
> Update this file whenever: (a) a new endpoint is probed, (b) a cassette diff trips, (c) a manual smoke run reveals a shape change.

## Auth

- Header: `Authorization: Token <api_key>`. Both `Bearer` and `Token` schemes were probed in earlier work — `Token` is confirmed working; `Bearer` returns 401.
- API keys are issued from the LingQ web UI under account settings.
- Keys never expire in observed behaviour (one key in use since 2024 still valid).
- No refresh / rotation endpoint discovered.

## Base URL & language tenancy

`https://www.lingq.com/api/v3/{lang}/...` where `{lang}` is an IETF-like segment (`ja`, `en`, `zh`, `ko`, `ru`, …). The language is part of the URL, not a header or a query. Cross-language calls (`/api/v3/en/…` against a Japanese collection ID) return 404. See AD-017.

## Endpoints (confirmed)

| Op | Method | URL | Notes |
|---|---|---|---|
| List my collections | GET | `/api/v3/{lang}/collections/my/?search=<title>&page_size=200` | Paginated; used to resume / dedupe by title. |
| Create collection | POST | `/api/v3/{lang}/collections/` | JSON body `{title, description}`. Returns `{id, …}`. Cover-image field shape **unconfirmed** — see Open probes. |
| List lessons | GET | `/api/v3/{lang}/collections/{cid}/lessons/?page=N&page_size=100` | Paginated. Used to skip already-uploaded lessons by title. |
| Import lesson | POST | `/api/v3/{lang}/lessons/import/` | multipart/form-data. Confirmed shape below. |

## `lessons/import/` multipart shape (confirmed)

```
title       string   chapter title
text        string   furigana-stripped chapter text (10k+ chars accepted)
collection  string   collection id (numeric, stringified)
language    string   matches {lang} URL segment (redundant but required)
level       string   "1".."6" (1 beginner-1 … 6 advanced-2)
status      string   "private" | "public"
tags        string   comma-separated, e.g. "books"
save        string   "true"
audio       file     audio/mpeg
```

Returns `{id, …}` on success. Lesson ID is the integer to thread into subsequent calls.

## Confirmed dead-ends

| URL | Behaviour |
|---|---|
| `/api/v3/contexts/` | 404. **Do not use** as an auth-check probe; use `/api/v3/{lang}/collections/my/?page_size=1` instead. |
| `/api/v2/*` | `400 {"detail": "API is obsolete. Use v3 instead."}` |

## Open probes

Each open probe needs timeboxed live-API exploration before the relevant feature lands. Probe order is significant — log status + body of each attempt.

### Cover image upload

Order of candidates:

1. Multipart `image` field on initial collection POST (`/api/v3/{lang}/collections/`).
2. `PATCH /api/v3/{lang}/collections/{cid}/` with `image` or `cover` form field.
3. `PATCH /api/v3/{lang}/collections/{cid}/image/` with `image` form field.

Log status + body of each. Document the winning shape here when known.

### Audio replacement on existing lesson

Order of candidates (from earlier probe scripts):

1. `PATCH /api/v3/{lang}/lessons/{id}/` (multipart `audio`).
2. `PUT /api/v3/{lang}/lessons/{id}/` (multipart `audio`).
3. `PATCH /api/v3/{lang}/lessons/{id}/audio/`.
4. `POST /api/v3/{lang}/lessons/{id}/audio/`.
5. `POST /api/v3/{lang}/lessons/{id}/upload-audio/`.

Document the winner here when probed.

## Error shapes (observed)

| Status | Shape | Cause |
|---|---|---|
| 200 / 201 | endpoint-specific JSON | success |
| 400 | `{"detail": "..."}` or `{"<field>": ["..."]}` | bad request, validation, or obsolete API |
| 401 | `{"detail": "Invalid token."}` | missing / wrong API key |
| 404 | `{"detail": "Not found."}` | wrong language URL, wrong collection ID, dead endpoint |
| 429 | not yet observed | rate limit (assume; add when seen) |
| 5xx | not yet observed | server error |

## Rate limits

Not documented; not yet probed in earnest. Manual smoke uploads (~70 lessons in a session) succeed without throttling. Assume per-second / per-minute soft limit; add backoff in `LingqClient` if 429 ever observed.
