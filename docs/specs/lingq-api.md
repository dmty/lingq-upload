# LingQ API — observed surface

> Status: **observed, not documented**. LingQ does not publish a v3 API reference. Every fact below was verified by probe against the live API and is subject to silent change. Schema-drift detection is handled by the cassette-based contract test strategy in AD-013.
>
> Update this file whenever: (a) a new endpoint is probed, (b) a cassette diff trips, (c) a manual smoke run reveals a shape change.

## Auth

- Header: `Authorization: Token <api_key>`. Both `Bearer` and `Token` schemes were probed in earlier work — `Token` is confirmed working; `Bearer` returns 401.
- API keys are issued from the LingQ web UI under account settings.
- Keys never expire in observed behaviour (one key in use since 2024 still valid).
- No refresh / rotation endpoint discovered.
- **User-Agent is required.** Requests with `python-urllib/*`, `curl/*` default, or empty UA are blocked by Cloudflare with `HTTP 403 error code: 1010` (browser signature ban). Send a real browser UA (e.g. `Mozilla/5.0 …Chrome/…`). The Rust `reqwest` default UA also passes; only headless/CLI defaults get flagged.

## Base URL & language tenancy

`https://www.lingq.com/api/v3/{lang}/...` where `{lang}` is an IETF-like segment (`ja`, `en`, `zh`, `ko`, `ru`, …). The language is part of the URL, not a header or a query. Cross-language calls (`/api/v3/en/…` against a Japanese collection ID) return 404. See AD-017.

## Endpoints (confirmed)

| Op | Method | URL | Notes |
|---|---|---|---|
| List my languages | GET | `/api/v2/languages/` | **One of the few surviving v2 endpoints.** Not lang-scoped. Returns the catalogue with the caller's known-word counts. Response is either a flat array or `{ results: [...] }`; the client tolerates either. |
| List my collections | GET | `/api/v3/{lang}/collections/my/?search=<title>&page_size=200` | Paginated; used to resume / dedupe by title. Entry is thin: `{id, title, imageUrl}`. |
| Collection detail | GET | `/api/v3/{lang}/collections/{cid}/` | Full course record. Fields listed below. |
| Create collection | POST | `/api/v3/{lang}/collections/` | JSON body `{title, description, tags?: ["books", …]}`. Returns `{id, …}`. `tags` is a JSON array (not comma-string as on lesson import). Cover-image field shape **unconfirmed** — see Open probes. There is **no `category` field**; LingQ's UI "Books" category is derived from the `books` tag. |
| List lessons | GET | `/api/v3/{lang}/collections/{cid}/lessons/?page=N&page_size=100` | Paginated. Used to skip already-uploaded lessons by title. Rich per-entry fields (see below). |
| Lesson detail | GET | `/api/v3/{lang}/lessons/{lid}/` | Full reader payload: `tokenizedText`, `translation`, `cards`, `words`, `bookmark`, reader/audio counters. |
| Lesson text | GET | `/api/v3/{lang}/lessons/{lid}/text/` | `{is_legacy, text, words}`. `words` is a dict of `{text, tags, importance, readings}`. |
| Lesson sentences | GET | `/api/v3/{lang}/lessons/{lid}/sentences/` | Array of `{index, text, cleanText, translations[], timestamp: [start,end], phrases, notes}`. Audio-aligned when audio is present. |
| Lesson words | GET | `/api/v3/{lang}/lessons/{lid}/words/` | `{cards: {…}}` — just the LingQ cards on the lesson, without the tokenized text. |
| Lesson bookmark | GET | `/api/v3/{lang}/lessons/{lid}/bookmark/` | `{wordIndex, completedWordIndex, audioPosition, client, timestamp}`. |
| List cards | GET | `/api/v3/{lang}/cards/?lesson={lid}&page_size=N` | Paginated. Card fields: `pk, term, fragment, status, extended_status, hints, notes, srs_due_date, last_reviewed_correct, readings, transliteration, audio, tags, url`. Without `lesson` filter, returns every card in the language (huge). |
| Import lesson | POST | `/api/v3/{lang}/lessons/import/` | multipart/form-data. Confirmed shape below. |

### `/api/v2/languages/` response (observed/permissive)

Per-entry fields, parsed permissively with these aliases:

| Logical | Accepted field names |
|---|---|
| `code` | `code`, `language`, `url_slug`, `tag` |
| `title` | `title`, `english_name`, `name`, `label` |
| `known_words` | `known_words`, `knownWords`, `words_known`, `wordsKnown` |

Confirm exact field names on first manual smoke; tighten the parser if the surface stabilises.

Full observed per-entry keys (v2/languages/): `id, url, code, title, supported, knownWords, lastUsed, grammar…`.

## Collection detail response (observed)

`GET /api/v3/{lang}/collections/{cid}/` — fields observed on a private user-owned course:

```
id                int
title             string
description       string
date              "YYYY-MM-DD"
level             string, e.g. "Intermediate 2"
difficulty        float
duration          int, sum of lesson durations in seconds
lessonsCount      int
newWordsCount     int
imageUrl          webp cover
originalImageUrl  orig-size cover
tags              string[], e.g. ["books"]
status            "private" | "public"
folders           int[]
sharedById        int
sharedByName      string
sharedByImageUrl  string
sharedByRole      string | null
type              "collection"
url               relative API url
isFeatured        bool
isTaken           bool
isSubscribed      bool
isLocked          bool | null
audioPending      bool
rosesCount        int
viewsCount        int
price             int
metadata          object | null
providerId        string | null
providerImageUrl  string | null
providerName      string | null
lessonsSortBy     string | null
source            string | null
accent            string | null
progress          object | null
```

## Lesson list / detail responses (observed)

**List entry** (`/collections/{cid}/lessons/`) — per lesson:

```
id, title, level, duration, wordCount, uniqueWordCount, newWordsCount,
audio (mp3 URL), imageUrl, originalImageUrl,
percentCompleted, listenTimes, viewsCount, canEdit, isProtected,
tags, shelves, status, date, collectionId, collectionTitle,
sharedById, sharedByName, providerImageUrl,
source: {type, name, url}, sourceType, sourceUrl,
type: "content", url, folders, accent, difficulty, price,
isFeatured, isOverLimit, lessonsSortBy, providerName, providerId, videoUrl
```

**Detail** (`/lessons/{lid}/`) — superset of list, adds the reader payload:

```
tokenizedText        [[{tokens: [{text, wordId, indexInSentence, transliteration, opentag?, closetag?}, …]}], …]
translation          {method: "chatgpt", language: "en", sentences: [...]}
cards                {pk: {pk, term, fragment, notes, status, hints, transliteration, gTags, importance, …}, …}
words                {wordId: {text, tags, importance, readings, status: "known"|…, hints: [...]}, …}
bookmark             {wordIndex, completedWordIndex, audioPosition, client, timestamp}
completed            bool
opened               bool
isFavorite           bool
lastOpenTime         ISO-8601
readTimes            float
listenTimes          float
percentCompleted     float
newWordsCount        int
uniqueWordCount      int
wordCount            int
cardsCount           int
audioUrl             final CDN URL (S3)
imageUrl             webp cover
originalImageUrl     orig-size cover
collection           {id, type, title, status, source, …}
nextLessonId, nextLesson: {id, title, image}
previousLessonId, previousLesson
pos                  int, position in collection
pubDate              "YYYY-MM-DD"
classicUrl           string   legacy reader URL
printUrl             string   printable URL
giveRoseUrl          string
canEdit, canEditSentence, isLegacy, isLocked, isOverLimit, isProtected,
audioPending, audioRating, audioVotes, lessonRating, lessonVotes,
scheduledForDeletion, roseGiven, rosesCount, viewsCount,
sharedById, sharedByName, sharedByImageUrl, sharedByIsFriend, sharedByRole,
promotedCourse, providerDescription, providerImageUrl, providerName, providerUrl,
simplifiedBy, simplifiedTo, external_type, videoUrl, metadata,
copyright, description, difficulty, duration, folders, level, price,
status: "D" (draft?) | …, tags, title, type: "lesson", url
```

Cover URLs (`imageUrl`, `originalImageUrl`) come back on both collection and lesson detail — no separate fetch endpoint is needed to display a cover once uploaded. Cover **upload** shape is still open (see below).

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
| `/api/v2/*` | `400 {"detail": "API is obsolete. Use v3 instead."}` — **except** `/api/v2/languages/` (see above). |
| `/api/v3/languages/` | 404. Use `/api/v2/languages/`. |
| `/api/v3/{lang}/stats/` | 404. No language-level stats endpoint. |
| `/api/v3/{lang}/collections/{cid}/{stats,progress,statistics,counters}/` | 404. No dedicated collection stats — counters are inline on the collection detail (`newWordsCount`, `lessonsCount`, `duration`, `viewsCount`, `rosesCount`). |
| `/api/v3/{lang}/lessons/{lid}/{stats,progress,statistics,counters,timings,audio,cards,translations,tokens}/` | 404. Reader progress lives on lesson detail (`percentCompleted`, `readTimes`, `listenTimes`, `bookmark`); text/translation/cards live on the confirmed sub-endpoints above. |
| `/api/v3/{lang}/cards/counters/` | 404 `{"detail":"Not found."}`. |

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
