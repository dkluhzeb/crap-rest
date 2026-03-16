# crap-rest

RESTful JSON gateway for [Crap CMS](https://github.com/your-org/crap-cms). Connects to the gRPC API as a client and re-exposes every RPC as a standard REST endpoint.

Separate binary, zero config required. Start it alongside `crap-cms serve` and talk JSON over HTTP instead of protobuf over gRPC.

## Quick Start

```bash
# Build
cargo build --release

# Start crap-cms first (in another terminal)
cd ../crap-cms && cargo run -- serve ./example

# Start the REST gateway
./target/release/crap-rest
# => listening on 0.0.0.0:8080, proxying to http://localhost:50051
```

## CLI Options

```
crap-rest [OPTIONS]

Options:
  -p, --port <PORT>    Listen port [default: 8080]
  -g, --grpc <ADDR>    gRPC server address [default: http://localhost:50051]
  -c, --config <FILE>  Config file path (optional)
  -h, --help           Print help
```

Examples:

```bash
# Custom port
crap-rest -p 3000

# Custom gRPC target
crap-rest -g http://192.168.1.10:50051

# With config file
crap-rest -c crap-rest.toml
```

## Config File

Optional TOML file. CLI flags override config values.

```toml
[server]
port = 8080
host = "0.0.0.0"

[grpc]
address = "http://localhost:50051"

[cors]
allowed_origins = ["*"]
# Or restrict:
# allowed_origins = ["https://myapp.com", "http://localhost:3000"]
```

## Logging

Uses `RUST_LOG` env var (via `tracing-subscriber`):

```bash
RUST_LOG=debug crap-rest         # verbose
RUST_LOG=crap_rest=debug crap-rest  # only gateway logs
```

## API Reference

All endpoints return JSON. Errors return `{ "error": "<message>" }` with appropriate HTTP status codes.

### Authentication

Pass a Bearer token in the `Authorization` header. The gateway forwards it to gRPC as-is — no JWT validation happens in the gateway.

```bash
curl -H 'Authorization: Bearer <token>' http://localhost:8080/collections/posts
```

---

### Collections

#### Find documents
```
GET /collections/:slug
```

Query parameters (all optional):

| Param      | Type    | Description                          |
|------------|---------|--------------------------------------|
| `where`    | string  | JSON filter expression               |
| `order_by` | string  | Sort field and direction              |
| `limit`    | integer | Max documents to return              |
| `offset`   | integer | Skip N documents (pagination)        |
| `depth`    | integer | Relationship population depth        |
| `locale`   | string  | Locale code                          |
| `select`   | string  | Comma-separated field names          |
| `draft`    | boolean | Include drafts                       |

```bash
curl 'http://localhost:8080/collections/posts?limit=10&offset=0&order_by=-created_at'
```

Response:
```json
{
  "docs": [
    { "id": "abc123", "collection": "posts", "title": "Hello", "created_at": "..." }
  ],
  "total": 42
}
```

#### Count documents
```
GET /collections/:slug/count
```

Query parameters: `where`, `locale`, `draft` (same as Find).

```bash
curl 'http://localhost:8080/collections/posts/count?where={"status":{"equals":"published"}}'
```

Response:
```json
{ "count": 42 }
```

#### Find by ID
```
GET /collections/:slug/:id
```

Query parameters: `depth`, `locale`, `select`, `draft`.

```bash
curl http://localhost:8080/collections/posts/abc123?depth=1
```

Response: flat document object (fields merged into top level).

#### Create document
```
POST /collections/:slug
```

```bash
curl -X POST http://localhost:8080/collections/posts \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"title": "New Post", "slug": "new-post", "content": "Hello world"}'
```

Special fields in the body (not stored, control behavior):
- `_locale` — target locale
- `_draft` — create as draft (`true`/`false`)

Response: the created document.

#### Update document
```
PATCH /collections/:slug/:id
```

```bash
curl -X PATCH http://localhost:8080/collections/posts/abc123 \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"title": "Updated Title"}'
```

Special body fields: `_locale`, `_draft`, `_unpublish`.

Response: the updated document.

#### Delete document
```
DELETE /collections/:slug/:id
```

```bash
curl -X DELETE http://localhost:8080/collections/posts/abc123 \
  -H 'Authorization: Bearer <token>'
```

Response:
```json
{ "success": true }
```

#### Bulk update
```
PATCH /collections/:slug/bulk
```

```bash
curl -X PATCH http://localhost:8080/collections/posts/bulk \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"where": "{\"status\":{\"equals\":\"draft\"}}", "data": {"status": "published"}}'
```

Response:
```json
{ "modified": 5 }
```

#### Bulk delete
```
DELETE /collections/:slug/bulk
```

```bash
curl -X DELETE http://localhost:8080/collections/posts/bulk \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"where": "{\"status\":{\"equals\":\"archived\"}}"}'
```

Response:
```json
{ "deleted": 3 }
```

---

### Globals

#### Get global
```
GET /globals/:slug
```

Query parameters: `locale`.

```bash
curl http://localhost:8080/globals/site-settings
```

#### Update global
```
PATCH /globals/:slug
```

```bash
curl -X PATCH http://localhost:8080/globals/site-settings \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"site_name": "My Site"}'
```

---

### Auth

#### Login
```
POST /auth/:collection/login
```

```bash
curl -X POST http://localhost:8080/auth/users/login \
  -H 'Content-Type: application/json' \
  -d '{"email": "admin@example.com", "password": "secret123"}'
```

Response:
```json
{
  "token": "eyJhbGciOi...",
  "user": { "id": "...", "email": "admin@example.com", ... }
}
```

#### Get current user
```
GET /auth/me
```

```bash
curl http://localhost:8080/auth/me \
  -H 'Authorization: Bearer eyJhbGciOi...'
```

#### Forgot password
```
POST /auth/:collection/forgot-password
```

```bash
curl -X POST http://localhost:8080/auth/users/forgot-password \
  -H 'Content-Type: application/json' \
  -d '{"email": "admin@example.com"}'
```

#### Reset password
```
POST /auth/:collection/reset-password
```

```bash
curl -X POST http://localhost:8080/auth/users/reset-password \
  -H 'Content-Type: application/json' \
  -d '{"token": "reset-token-here", "new_password": "newpass123"}'
```

#### Verify email
```
POST /auth/:collection/verify-email
```

```bash
curl -X POST http://localhost:8080/auth/users/verify-email \
  -H 'Content-Type: application/json' \
  -d '{"token": "verification-token-here"}'
```

---

### Schema

#### List all collections and globals
```
GET /schema
```

```bash
curl http://localhost:8080/schema
```

Response:
```json
{
  "collections": [
    { "slug": "posts", "singular_label": "Post", "plural_label": "Posts", "timestamps": true, "auth": false, "upload": false }
  ],
  "globals": [
    { "slug": "site-settings", "singular_label": "Site Settings", "plural_label": null }
  ]
}
```

#### Describe collection
```
GET /schema/collections/:slug
```

```bash
curl http://localhost:8080/schema/collections/posts
```

Returns full field definitions including types, validation, relationships, and blocks.

#### Describe global
```
GET /schema/globals/:slug
```

---

### Versions

#### List versions
```
GET /collections/:slug/:id/versions
```

Query parameters: `limit`.

```bash
curl http://localhost:8080/collections/posts/abc123/versions?limit=10 \
  -H 'Authorization: Bearer <token>'
```

Response:
```json
{
  "versions": [
    { "id": "v1", "version": 1, "status": "published", "latest": true, "created_at": "..." }
  ]
}
```

#### Restore version
```
POST /collections/:slug/:id/versions/:vid/restore
```

```bash
curl -X POST http://localhost:8080/collections/posts/abc123/versions/v1/restore \
  -H 'Authorization: Bearer <token>'
```

---

### Jobs

#### List job definitions
```
GET /jobs
```

#### Trigger a job
```
POST /jobs/:slug/trigger
```

```bash
curl -X POST http://localhost:8080/jobs/send-newsletter/trigger \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"data": {"template": "weekly"}}'
```

Response:
```json
{ "job_id": "run_abc123" }
```

#### Get job run status
```
GET /jobs/runs/:id
```

#### List job runs
```
GET /jobs/runs
```

Query parameters: `slug`, `status`, `limit`, `offset`.

---

## Error Responses

All errors return JSON with an appropriate HTTP status:

| gRPC Status         | HTTP Status |
|---------------------|-------------|
| `NOT_FOUND`         | 404         |
| `INVALID_ARGUMENT`  | 400         |
| `PERMISSION_DENIED` | 403         |
| `UNAUTHENTICATED`   | 401         |
| `ALREADY_EXISTS`    | 409         |
| `UNAVAILABLE`       | 503         |
| Other               | 500         |

```json
{ "error": "document not found" }
```

## Architecture

```
Browser/App ──HTTP/JSON──▶ crap-rest ──gRPC──▶ crap-cms
                           (port 8080)         (port 50051)
```

- Stateless proxy — no database, no auth logic, no sessions
- Lazy gRPC connection — starts even if crap-cms isn't running yet
- Auth tokens forwarded as-is via gRPC metadata
- CORS enabled by default (configurable)
- Response compression (gzip + brotli)
