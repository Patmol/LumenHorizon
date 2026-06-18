# 30. Local Development And CI

## Goals

- Keep a clean clone easy to run locally.
- Keep `just` commands and CI aligned.
- Validate backend behavior without remote credentials.
- Build and scan local backend images without publishing them.

## Local Commands

| Command | Purpose |
| --- | --- |
| `just setup` | Create `.env` when needed and check local tools. |
| `just up` | Start PostgreSQL and Azurite. |
| `just down` | Stop local dependencies. |
| `just migrate` | Apply migrations. |
| `just serve` | Start `ingest-svc`. |
| `just serve-api` | Start `api-gateway`. |
| `just check` | Run Rust fmt/check/clippy/test. |
| `just openapi-check` | Verify OpenAPI route/auth contract. |
| `just validate` | Run active local validation stack. |

## CI Jobs

The active CI workflow has two jobs:

- Rust: formatting, `cargo check`, clippy, OpenAPI contract check, and workspace tests.
- Docker: build and scan `ingest-svc`, `db-migrate`, `processing-svc`, and `api-gateway` images with generic `lumenhorizon/*:<commit-sha>` tags.

CI must not require remote credentials. It should be reproducible from local commands where practical.

## Docker Images

Local image targets:

```bash
just docker-build
just docker-build-db-migrate
just docker-build-processing
just docker-build-api-gateway
```

Images are local build artifacts used for validation and runtime packaging checks.

## Validation Standard

Before marking backend work complete, run the narrowest useful check plus the broader stack when the change affects shared contracts:

```bash
just openapi-check
just validate
```

For Rust-only changes:

```bash
cd backend
cargo fmt --all -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
