# Alloy sync server

An optional, zero-knowledge blob store for Alloy's own state (ADR 0020:
`docs/decisions/0020-server-primary-sync.md`). It stores opaque bytes under opaque
names and enforces one compare-and-swap chain. It has no build step, no runtime
dependencies, and no knowledge of the client's key hierarchy or data model.

Alloy works fully without it. The server is additive storage, never an identity
provider: it cannot enrol a device, cannot issue a key, and cannot read what it holds.

```
server/
  src/core.js          one request handler, Web Fetch API signature
  src/bytes.js         sha256, constant-time compare, encoding
  src/tokens.js        token/quota spec parsing
  src/version.js       version reported by /healthz
  src/memory-store.js  in-memory Store adapter (tests)
  src/disk-store.js    directory-on-disk Store adapter (self-hosting)
  src/kv-store.js      Workers KV Store adapter (hosted mode)
  src/node.js          Node http <-> Web Request/Response adapter
  src/main.js          self-hosted entry point
  src/worker.js        Cloudflare Worker entry
  test/                node --test suites
```

Routing, limits, authentication and storage decisions live in `core.js` only. The
Worker entry and both Node paths call the same handler; adapters only move bytes.

## What the server can and cannot see

Can see:

- the HTTP method, the URL path, and therefore the blob id and which of `head`,
  `escrow` and `roster` was read or written;
- body sizes and request timing;
- which token authenticated the request (one token = one quota identity), and the
  token value itself, in memory, for the duration of the comparison;
- its own stored ciphertext.

Cannot see:

- plaintext, keys, or any client key material;
- what a blob is, which layer it belongs to, or which device wrote it;
- plaintext identifiers. Blob ids are `HMAC(addr_key, logical_id)` on the client, so
  the id is already unlinkable to a name; the server validates the shape (`43`
  base64url characters) and never decodes, sorts, or interprets it;
- anything inside `head`, `escrow` or `roster`: they are byte strings to it.

It also cannot fork or roll back silently: `head` is written only through
compare-and-swap, and the client verifies the signed chain it receives. A server
that mutates or replays data is detected by the client, not by this process.

Residual metadata that no design removes here: request timing, body sizes, and the
fact that two requests used the same token. The client pads bodies to fixed size
classes; this server does not add padding of its own.

## Wire contract

All bodies are `application/octet-stream`. All resources except `/healthz` need
`Authorization: Bearer <token>`, compared in constant time against every configured
token.

| Method and path | Behaviour |
|---|---|
| `GET /healthz` | `200 {"ok":true,"version":"..."}`. No auth, no configuration details. |
| `GET /v1/head`, `/v1/escrow`, `/v1/roster` | `200` with the bytes and an `ETag`; `404` if never written. |
| `PUT /v1/head`, `/v1/escrow`, `/v1/roster` | Compare-and-swap write, `204` on success. |
| `GET /v1/blob/{id}` | `200` with the bytes and an `ETag`; `404` if absent. |
| `PUT /v1/blob/{id}` | Overwrite-allowed write, `204` on success. |
| `DELETE /v1/blob/{id}` | `204` when it existed and was removed; `404` when it is already absent, so a retried delete is not an error. The client names every id it deletes; there is no enumeration endpoint. |

Deleting a blob does **not** refund the token's quota: the bytes were already accounted when they
arrived, and refunding would let a client rewrite and delete in a loop for free. A delete is the
only way ciphertext leaves the server (revocation, ADR 0020 invariant 14), so it is a real
deletion of the stored bytes, not a tombstone the store may protect from overwrite.

CAS preconditions on the three singleton resources:

- `If-Match: "<etag>"` — write only if the current `ETag` is exactly this one. A stale
  value returns `409` whose body is the *current* bytes and whose `ETag` header is the
  current tag, so the caller can rebase and retry.
- `If-None-Match: *` — create only: succeeds while the resource does not exist,
  returns `412` once it does.
- Neither header returns `428`.

Status codes: `400` malformed request or blob id, `401` missing or wrong token,
`404` unknown route or absent resource, `405` wrong method (`Allow` header set),
`409` stale `If-Match` (with the current body), `412` failed `If-None-Match`, `413`
body over the cap, `415` wrong `Content-Type`, `428` missing precondition, `429`
quota exceeded, `500` internal error.

Hard limits, applied before anything is written:

- **1 MiB per body** (`413`), enforced while reading: a body whose `Content-Length`
  claims more is rejected without a read, and a stream that keeps going past the cap
  is cancelled rather than buffered.
- **`Content-Type: application/octet-stream`** on every `PUT` (`415`).
- **Blob ids** must match `^[A-Za-z0-9_-]{43}$` (`400` otherwise). A traversal-looking
  id never reaches a storage adapter; the disk adapter additionally rejects any key
  that is not plain path segments, so a bug cannot escape its root.
- **Quota per token** (`429`).

Logging is off by default. With `SYNC_LOG=1` the server logs the method, status and
duration of each request, and nothing else: no path, id, body, token or header value.

## Deployment

### 1. Cloudflare Workers + KV (free tier, `prefs` layer)

Requires a KV namespace. No server process, no disk.

```sh
cd server
npx wrangler kv namespace create SYNC_KV      # copy the id into wrangler.toml
npx wrangler secret put SYNC_TOKENS           # e.g. token1,token2:67108864
npx wrangler deploy
```

`wrangler.toml` binds KV as `SYNC_KV` and sets `SYNC_LOG = "0"`. Workers provides
TLS. KV is eventually consistent: two devices writing `head` from different edges can
both believe they won for a short window, and the client detects the fork from the
signed chain on its next read. Use this mode for the `prefs` layer only; the ADR rules
out Workers for the growing message/media archive.

### 2. Docker

```sh
docker build -t alloy-sync ./server
docker run -d --name alloy-sync -p 127.0.0.1:8080:8080 \
  -e SYNC_TOKENS=token1,token2:67108864 \
  -v alloy-sync-data:/data \
  alloy-sync
```

The container listens on `0.0.0.0:8080`, stores under `/data`, and runs as `node`
(uid 1000). It speaks plain HTTP: put a TLS-terminating reverse proxy in front.

### 3. Plain Node behind a reverse proxy

Node 20 or newer, nothing to install:

```sh
SYNC_DATA_DIR=/var/lib/alloy-sync \
SYNC_TOKENS_FILE=/etc/alloy-sync/tokens \
PORT=8080 HOST=127.0.0.1 \
node server/src/main.js
```

Example nginx front end (TLS terminated here; the backend is bound to loopback):

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $host;
    proxy_request_buffering off;
    client_max_body_size 2m;
}
```

Caddy works the same way with `reverse_proxy 127.0.0.1:8080`. TLS 1.3 only is the
client's expectation (ADR 0020 invariant 9); configure the proxy accordingly.

### Configuration

| Variable | Meaning |
|---|---|
| `SYNC_DATA_DIR` | Required for the Node entry: directory holding the blobs. |
| `SYNC_TOKENS` | Comma separated `token` or `token:quota_bytes`. |
| `SYNC_TOKENS_FILE` | Alternative to `SYNC_TOKENS`: one entry per line, `#` comments allowed. |
| `SYNC_LOG` | `1` enables the access log (method, status, duration). |
| `PORT`, `HOST` | Node entry only; defaults `8080`, `0.0.0.0`. |
| `SYNC_KV`, `SYNC_TOKENS` | Worker bindings/vars instead of the above. |

Generate a token with 32 random bytes:

```sh
openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
# or: node -e "console.log(require('crypto').randomBytes(32).toString('base64url'))"
```

One token per device is the intended shape: each device then has its own quota and can
be revoked by deleting its entry and restarting the server. Rotating a token means
editing the list and restarting; there is no API for it, and a stolen token is only as
useful as the ciphertext it can fetch, which it cannot decrypt.

Quota is a byte budget per token, default 256 MiB (`268435456`). It counts every byte
that token has written — blobs, `head`, `escrow`, `roster` — and is monotonic: there is
no delete endpoint, so a rewrite of the same 1 MiB blob charges 1 MiB again. Size the
quota for the total upload volume a device will ever produce, not for its peak
footprint; on a hosted free tier this also keeps KV operations predictable. The running
total per token is stored as `usage/<sha256(token)>`, so the token itself is never
written to disk.

## Tests

```sh
cd server
node --test
```

Covers: CAS conflict returns `409` and the losing write does not land; bad and missing
tokens are `401`; oversized bodies are `413` while the server stops reading; quota is
`429`; the disk adapter roundtrip including delete and prefix list; concurrent `PUT`s
of the same head have exactly one winner; the access log contains no id, path or token;
traversal-looking ids are rejected before any adapter; and both the Node and Worker
entries run the same core handler over real transports.

## Backup

Everything the server holds is ciphertext with opaque names, so a backup is a file copy:

- Node and Docker: copy or snapshot `SYNC_DATA_DIR`. Stop the container, or copy the
  directory and accept that a `PUT` in flight during the copy may be missing; a blob is
  written to `.tmp` and renamed, so a copy never contains a half-written file. Restore
  by placing the directory back and starting the process.
- Workers: use KV's own export/import tooling (`wrangler kv bulk get`/`put`) or your
  Cloudflare account's backup process.

A backup is only useful with the device-held master key: the operator can restore the
bytes, never read them.

## Losing the server

Losing the server is not losing data. Each enrolled device keeps its full local
SQLCipher database and runs offline exactly as it does with no server configured
(ADR 0020 invariant 6); the server is authoritative only for what it stores, and only
for layers the user chose to mirror. A server that is destroyed, wiped, or withheld
costs availability and, at worst, the changes made on other devices since the last
pull. The one case that cannot be recovered from the server side is losing *every*
device — which is why recovery is a 24-word phrase that wraps the master key into the
`escrow` blob, opt-in and off by default. Without that phrase, recovery needs a device
that still exists, and the ciphertext on this server is unreadable forever, including
by whoever runs it.
