#!/usr/bin/env bash
# Rename project scaffolding from "platform" to a name you choose.
#
# Replaces crate names, paths, migration tables, Redpanda topics, docker defaults, etc.
# Preserves MySo domain terms: PLATFORM_ID, platform_id columns, platforms table.
#
# Usage (from anywhere):
#   ./scripts/rename_project.sh
#
# Run once on a fresh fork. Review the diff afterward and run: cargo build && cargo test

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OLD_SLUG="platform"
OLD_SNAKE="platform"

echo "Platform Server Template — project renamer"
echo "Root: $ROOT"
echo
read -rp "Enter new project name (e.g. dripdrop, acme-social): " RAW_NAME

SLUG="$(printf '%s' "$RAW_NAME" \
  | tr '[:upper:]' '[:lower:]' \
  | tr ' _.' '-' \
  | sed 's/[^a-z0-9-]//g' \
  | sed 's/-\{2,\}/-/g' \
  | sed 's/^-//;s/-$//')"

if [[ -z "$SLUG" ]]; then
  echo "Error: project name is empty after normalization." >&2
  exit 1
fi

if [[ "$SLUG" == "$OLD_SLUG" ]]; then
  echo "Error: choose a name other than '$OLD_SLUG'." >&2
  exit 1
fi

if [[ ! "$SLUG" =~ ^[a-z][a-z0-9-]*$ ]]; then
  echo "Error: use lowercase letters, numbers, and hyphens; start with a letter." >&2
  exit 1
fi

SNAKE="${SLUG//-/_}"
TITLE="$(printf '%s' "$SLUG" | sed 's/-/ /g' | awk '{
  for (i = 1; i <= NF; i++) {
    $i = toupper(substr($i, 1, 1)) tolower(substr($i, 2))
  }
  print
}')"

echo
echo "Will apply:"
echo "  kebab-case (crates, binaries): $SLUG-*"
echo "  snake_case (Rust crates, SQL): ${SNAKE}_*"
echo "  title (README heading):        ${TITLE} Server Template"
echo
echo "Unchanged (MySo / chain domain): PLATFORM_ID, platform_id, platforms table"
echo
read -rp "Proceed with rename? [y/N] " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 0
fi

replace_in_file() {
  local file="$1"
  perl -pi -e "
    s/\QPlatform Server Template\E/${TITLE} Server Template/g;
    s/\Qplatform-server-template\E/${SLUG}-server-template/g;
    s/\Qplatform-server\E/${SLUG}-server/g;
    s/\Qplatform-analytics\E/${SLUG}-analytics/g;
    s/\Qplatform-indexer\E/${SLUG}-indexer/g;
    s/\Qplatform-notify\E/${SLUG}-notify/g;
    s/\Qplatform-core\E/${SLUG}-core/g;
    s/\Qplatform-api\E/${SLUG}-api/g;
    s/\Qplatform-db\E/${SLUG}-db/g;
    s/\Qplatform_schema_migrations\E/${SNAKE}_schema_migrations/g;
    s/\Qplatform_delivery_config\E/${SNAKE}_delivery_config/g;
    s/\Qplatform_analytics\E/${SNAKE}_analytics/g;
    s/\Qplatform_indexer\E/${SNAKE}_indexer/g;
    s/\Qplatform_notify\E/${SNAKE}_notify/g;
    s/\Qplatform_core\E/${SNAKE}_core/g;
    s/\Qplatform_api\E/${SNAKE}_api/g;
    s/\Qplatform_db\E/${SNAKE}_db/g;
    s/\Qplatform_filter\E/${SNAKE}_filter/g;
    s/\Qplatform_events\E/${SNAKE}_events/g;
    s/\Qplatform.chain.events\E/${SLUG}.chain.events/g;
    s/\Qplatform.stream.events\E/${SLUG}.stream.events/g;
    s/\Qplatform.api.logs\E/${SLUG}.api.logs/g;
    s/\Qplatform.notifications\E/${SLUG}.notifications/g;
    s/postgres:\\/\\/${OLD_SLUG}:${OLD_SLUG}@/postgres:\\/\\/${SLUG}:${SLUG}@/g;
    s/POSTGRES_USER: ${OLD_SLUG}/POSTGRES_USER: ${SLUG}/g;
    s/POSTGRES_PASSWORD: ${OLD_SLUG}/POSTGRES_PASSWORD: ${SLUG}/g;
    s/POSTGRES_DB: ${OLD_SLUG}/POSTGRES_DB: ${SLUG}/g;
    s/pg_isready -U ${OLD_SLUG} -d ${OLD_SLUG}/pg_isready -U ${SLUG} -d ${SLUG}/g;
    s|\\/tmp\\/${OLD_SLUG}-|/tmp/${SLUG}-|g;
    s/useradd --system --create-home --uid 1001 ${OLD_SLUG}/useradd --system --create-home --uid 1001 ${SLUG}/g;
    s/\\/usr\\/local\\/bin\\/${OLD_SLUG}-server/\\/usr\\/local\\/bin\\/${SLUG}-server/g;
    s/chown -R ${OLD_SLUG}:${OLD_SLUG}/chown -R ${SLUG}:${SLUG}/g;
    s/^USER ${OLD_SLUG}\$/USER ${SLUG}/gm;
  " "$file"
}

echo "Updating file contents..."
while IFS= read -r -d '' file; do
  replace_in_file "$file"
done < <(
  find "$ROOT" -type f \
    ! -path '*/target/*' \
    ! -path '*/.git/*' \
    ! -path '*/scripts/rename_project.sh' \
    \( \
      -name '*.rs' -o -name '*.toml' -o -name '*.sql' -o -name '*.md' -o -name '*.sh' \
      -o -name '*.yml' -o -name '*.yaml' -o -name '*.example' -o -name 'Dockerfile' \
      -o -name 'Cargo.lock' \
    \) -print0
)

echo "Renaming source files..."
while IFS= read -r -d '' file; do
  dir="$(dirname "$file")"
  base="$(basename "$file")"
  new_base="${base/${OLD_SNAKE}_filter/${SNAKE}_filter}"
  if [[ "$base" != "$new_base" ]]; then
    mv "$file" "$dir/$new_base"
  fi
done < <(find "$ROOT/crates" -type f -name "${OLD_SNAKE}_filter.rs" -print0 2>/dev/null || true)

echo "Renaming crate directories..."
CRATE_DIRS=(
  "${OLD_SLUG}-core"
  "${OLD_SLUG}-db"
  "${OLD_SLUG}-indexer"
  "${OLD_SLUG}-api"
  "${OLD_SLUG}-notify"
  "${OLD_SLUG}-analytics"
  "${OLD_SLUG}-server"
)
for old in "${CRATE_DIRS[@]}"; do
  src="$ROOT/crates/$old"
  dst="$ROOT/crates/${old/$OLD_SLUG/$SLUG}"
  if [[ -d "$src" ]]; then
    mv "$src" "$dst"
  fi
done

if [[ "$(basename "$ROOT")" == "${OLD_SLUG}-server-template" ]]; then
  echo
  read -rp "Rename repo folder to ${SLUG}-server-template? [y/N] " RENAME_ROOT
  if [[ "$RENAME_ROOT" =~ ^[Yy]$ ]]; then
    PARENT="$(dirname "$ROOT")"
    NEW_ROOT="$PARENT/${SLUG}-server-template"
    if [[ -e "$NEW_ROOT" ]]; then
      echo "Error: destination already exists: $NEW_ROOT" >&2
      exit 1
    fi
    mv "$ROOT" "$NEW_ROOT"
    ROOT="$NEW_ROOT"
  fi
fi

echo
echo "Done."
echo
echo "Next steps:"
echo "  cd $(printf '%q' "$ROOT")"
echo "  rm -rf target   # stale build artifacts"
echo "  cargo build"
echo "  cargo test"
echo
echo "If this repo lives inside ProjectYZ, update sibling paths in README/Dockerfile if you renamed the root folder."
