if [[ -z "${ROOT_DIR:-}" ]]; then
  ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

POSTGRES_CONTAINER_NAME="lumenhorizon-postgres"
AZURITE_CONTAINER_NAME="lumenhorizon-azurite"
POSTGRES_VOLUME_NAME="lumenhorizon-postgres-data"
AZURITE_VOLUME_NAME="lumenhorizon-azurite-data"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: required command '$1' was not found." >&2
    exit 1
  fi
}

require_docker_runtime() {
  require_command docker

  if ! docker compose version >/dev/null 2>&1; then
    echo "Error: Docker Compose v2 is required." >&2
    exit 1
  fi
}

require_apple_container_runtime() {
  require_command container

  if ! container system status >/dev/null 2>&1; then
    echo "Error: Apple container services are not running. Run 'container system start' or set LUMENHORIZON_CONTAINER_RUNTIME=docker." >&2
    exit 1
  fi
}

container_runtime() {
  local requested_runtime="${LUMENHORIZON_CONTAINER_RUNTIME:-auto}"

  case "${requested_runtime}" in
    auto|"")
      if command -v container >/dev/null 2>&1 && container system status >/dev/null 2>&1; then
        echo "container"
        return
      fi

      if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
        echo "docker"
        return
      fi

      echo "Error: no supported container runtime is available. Install Apple container or Docker Desktop with Compose v2." >&2
      exit 1
      ;;
    container)
      require_apple_container_runtime
      echo "container"
      ;;
    docker)
      require_docker_runtime
      echo "docker"
      ;;
    *)
      echo "Error: LUMENHORIZON_CONTAINER_RUNTIME must be 'auto', 'container', or 'docker'." >&2
      exit 1
      ;;
  esac
}

container_runtime_label() {
  case "$1" in
    container)
      echo "Apple container"
      ;;
    docker)
      echo "Docker Compose"
      ;;
    *)
      echo "$1"
      ;;
  esac
}

require_container_runtime() {
  container_runtime >/dev/null
}

require_env_file() {
  if [[ ! -f "${ROOT_DIR}/.env" ]]; then
    echo "Error: .env was not found. Run ./scripts/setup-local.sh first."
    exit 1
  fi
}

load_env() {
  require_env_file

  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/.env"
  set +a
}

wait_for_postgres() {
  local timeout_seconds="${1:-60}"
  local elapsed_seconds=0
  local runtime_choice
  runtime_choice="$(container_runtime)"

  echo "==> Waiting for PostgreSQL..."
  until postgres_is_ready "${runtime_choice}"; do
    if (( elapsed_seconds >= timeout_seconds )); then
      echo "Error: PostgreSQL was not ready after ${timeout_seconds}s. Run 'just up' and check local dependency logs."
      exit 1
    fi

    sleep 1
    elapsed_seconds=$((elapsed_seconds + 1))
  done
}

postgres_is_ready() {
  local runtime_choice="$1"

  case "${runtime_choice}" in
    container)
      container exec "${POSTGRES_CONTAINER_NAME}" pg_isready -U lumenhorizon -d lumenhorizon >/dev/null 2>&1
      ;;
    docker)
      docker compose --project-directory "${ROOT_DIR}" exec -T postgres pg_isready -U lumenhorizon -d lumenhorizon >/dev/null 2>&1
      ;;
    *)
      echo "Error: unsupported container runtime '${runtime_choice}'." >&2
      exit 1
      ;;
  esac
}

ensure_apple_container_volume() {
  local volume_name="$1"

  if ! container volume inspect "${volume_name}" >/dev/null 2>&1; then
    container volume create "${volume_name}" >/dev/null
  fi
}

remove_apple_container_if_exists() {
  local container_name="$1"

  if container inspect "${container_name}" >/dev/null 2>&1; then
    container delete --force "${container_name}" >/dev/null
  fi
}

start_local_dependencies() {
  local runtime_choice
  runtime_choice="$(container_runtime)"

  echo "==> Starting local dependencies with $(container_runtime_label "${runtime_choice}")..."

  case "${runtime_choice}" in
    container)
      ensure_apple_container_volume "${POSTGRES_VOLUME_NAME}"
      ensure_apple_container_volume "${AZURITE_VOLUME_NAME}"
      remove_apple_container_if_exists "${POSTGRES_CONTAINER_NAME}"
      remove_apple_container_if_exists "${AZURITE_CONTAINER_NAME}"

      container run --detach \
        --name "${POSTGRES_CONTAINER_NAME}" \
        --env POSTGRES_USER=lumenhorizon \
        --env POSTGRES_PASSWORD=lumenhorizon \
        --env POSTGRES_DB=lumenhorizon \
        --env PGDATA=/var/lib/postgresql/data/pgdata \
        --publish 5432:5432 \
        --mount "type=volume,source=${POSTGRES_VOLUME_NAME},target=/var/lib/postgresql/data" \
        postgres:16-alpine >/dev/null

      container run --detach \
        --name "${AZURITE_CONTAINER_NAME}" \
        --publish 10000:10000 \
        --publish 10001:10001 \
        --mount "type=volume,source=${AZURITE_VOLUME_NAME},target=/data" \
        mcr.microsoft.com/azure-storage/azurite:3.32.0 \
        azurite --blobHost 0.0.0.0 --queueHost 0.0.0.0 --skipApiVersionCheck >/dev/null
      ;;
    docker)
      docker compose --project-directory "${ROOT_DIR}" up -d postgres azurite
      ;;
    *)
      echo "Error: unsupported container runtime '${runtime_choice}'." >&2
      exit 1
      ;;
  esac
}

stop_local_dependencies() {
  local runtime_choice
  runtime_choice="$(container_runtime)"

  echo "==> Stopping local dependencies with $(container_runtime_label "${runtime_choice}")..."

  case "${runtime_choice}" in
    container)
      remove_apple_container_if_exists "${POSTGRES_CONTAINER_NAME}"
      remove_apple_container_if_exists "${AZURITE_CONTAINER_NAME}"
      ;;
    docker)
      docker compose --project-directory "${ROOT_DIR}" down
      ;;
    *)
      echo "Error: unsupported container runtime '${runtime_choice}'." >&2
      exit 1
      ;;
  esac
}

container_image_build() {
  local dockerfile_path="$1"
  local image_tag="$2"
  local context_dir="${3:-.}"
  local runtime_choice
  runtime_choice="$(container_runtime)"

  echo "==> Building ${image_tag} with $(container_runtime_label "${runtime_choice}")..."

  case "${runtime_choice}" in
    container)
      container build --pull --file "${dockerfile_path}" --tag "${image_tag}" "${context_dir}"
      ;;
    docker)
      docker build --pull --file "${dockerfile_path}" --tag "${image_tag}" "${context_dir}"
      ;;
    *)
      echo "Error: unsupported container runtime '${runtime_choice}'." >&2
      exit 1
      ;;
  esac
}