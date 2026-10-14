# Optional build-phase timing shared by the Linux and macOS Preview packagers.
# Set AOS_RELEASE_COST_TIMINGS to a writable TSV path to enable it.

_AOS_RELEASE_COST_PHASES=()
_AOS_RELEASE_COST_STARTS=()

aos_release_cost_start() {
  [ -n "${AOS_RELEASE_COST_TIMINGS:-}" ] || return 0
  local depth=${#_AOS_RELEASE_COST_PHASES[@]}
  _AOS_RELEASE_COST_PHASES[$depth]="$1"
  _AOS_RELEASE_COST_STARTS[$depth]="$SECONDS"
}

aos_release_cost_end() {
  [ -n "${AOS_RELEASE_COST_TIMINGS:-}" ] || return 0
  local depth=$((${#_AOS_RELEASE_COST_PHASES[@]} - 1))
  [ "$depth" -ge 0 ] || return 0
  local phase="${_AOS_RELEASE_COST_PHASES[$depth]}"
  local start="${_AOS_RELEASE_COST_STARTS[$depth]}"
  local duration_seconds=$((SECONDS - start))
  mkdir -p "$(dirname "$AOS_RELEASE_COST_TIMINGS")"
  printf '%s\t%s\n' "$phase" "$((duration_seconds * 1000))" \
    >> "$AOS_RELEASE_COST_TIMINGS"
  unset "_AOS_RELEASE_COST_PHASES[$depth]"
  unset "_AOS_RELEASE_COST_STARTS[$depth]"
}
