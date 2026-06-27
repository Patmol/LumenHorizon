set shell := ["bash", "-cu"]

# List available commands
default:
    @just --list

# Create .env and check local tools
setup:
    ./scripts/setup-local.sh

# Start local PostgreSQL and Azurite
up:
    ./scripts/dev-up.sh

# Apply local database migrations
migrate:
    ./scripts/dev-migrate.sh

# Start the ingest HTTP service locally
serve:
    ./scripts/dev-serve.sh

# Start the API Gateway locally
serve-api:
    ./scripts/dev-api-gateway.sh

# Run one controlled raw blob ingest for a cadence: daily or monthly
ingest cadence="daily":
    ./scripts/dev-ingest.sh {{cadence}}

# Run the processing worker until the visible queue is empty
processing:
    cd backend && set -a && source ../.env && set +a && cargo run -p processing-svc -- worker

# Recover validated/downloaded ingest rows and pending enqueue outbox records
recover-ingest:
    cd backend && set -a && source ../.env && set +a && cargo run -p ingest-svc -- recover-ingest

# Replay one rejected ingest row by id
replay-rejected ingest_id:
    cd backend && set -a && source ../.env && set +a && cargo run -p ingest-svc -- replay-rejected {{ingest_id}}

# Publish a mosaic from processed granules; omit dataset_date or pass latest for the newest eligible date
publish-mosaic product dataset_date="" public_latest="false" allow_incomplete="false":
    cd backend && set -a && source ../.env && set +a && dataset_date_arg="{{dataset_date}}" && public_latest_arg="{{public_latest}}" && allow_incomplete_arg="{{allow_incomplete}}" && if [[ "$dataset_date_arg" == "true" || "$dataset_date_arg" == "false" ]]; then allow_incomplete_arg="$public_latest_arg"; public_latest_arg="$dataset_date_arg"; dataset_date_arg=""; fi && mosaic_args=("{{product}}") && if [[ -n "$dataset_date_arg" && "$dataset_date_arg" != "latest" ]]; then mosaic_args+=("$dataset_date_arg"); fi && if [[ "$public_latest_arg" == "true" ]]; then mosaic_args+=(--public-latest); fi && if [[ "$allow_incomplete_arg" == "true" ]]; then mosaic_args+=(--allow-incomplete-public-latest); fi && cargo run -p processing-svc -- publish-mosaic "${mosaic_args[@]}"

# Preview retention cleanup selections without deleting blobs
retention-cleanup:
    cd backend && set -a && source ../.env && set +a && cargo run -p processing-svc -- retention-cleanup

# Execute retention cleanup deletes selected stale blobs
retention-cleanup-execute:
    cd backend && set -a && source ../.env && set +a && cargo run -p processing-svc -- retention-cleanup --execute

# Stop local dependencies
down:
    ./scripts/dev-down.sh

# Run Rust formatting, workspace checks, and tests
check:
    cd backend && cargo fmt --all -- --check
    cd backend && cargo check
    cd backend && cargo clippy --workspace --all-targets -- -D warnings
    cd backend && cargo test --workspace

# Validate the API Gateway OpenAPI contract against implemented /api/v1 routes
openapi-check:
    cd backend && cargo test -p api-gateway openapi_contract_covers_gateway_api_routes

# Build the local ingest service Docker image
docker-build:
    . ./scripts/lib.sh; container_image_build backend/ingest-svc/Dockerfile ingest-svc:local .

# Build the local database migration Docker image
docker-build-db-migrate:
    . ./scripts/lib.sh; container_image_build backend/db-migrate/Dockerfile db-migrate:local .

# Build the local processing service Docker image
docker-build-processing:
    . ./scripts/lib.sh; container_image_build backend/processing-svc/Dockerfile processing-svc:local .

# Build the local API Gateway Docker image
docker-build-api-gateway:
    . ./scripts/lib.sh; container_image_build backend/api-gateway/Dockerfile api-gateway:local .

# Run local equivalents for current CI checks
ci-local:
    just check
    just docker-build
    just docker-build-db-migrate
    just docker-build-processing
    just docker-build-api-gateway

# Run non-secret local checks for CI and API contract validation
validate:
    just openapi-check
    just ci-local

# Run the local health smoke check
health:
    @if [[ ! -f .env ]]; then echo "Error: .env was not found. Run just setup first."; exit 1; fi; set -a; source .env; set +a; curl --fail "http://localhost:${PORT:-8083}/health"
