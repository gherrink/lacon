#!/usr/bin/env bash
# PostToolUse probe: tests whether Claude Code honors various output-rewrite
# channels. Fires ONLY when the tool command contains LACON_PROBE_MARKER, so
# every other Bash call passes through untouched.
#
# What we return: JSON with three different fields that *might* let us replace
# what the model sees as the tool result:
#   - hookSpecificOutput.updatedToolOutput   (claimed by some research, not in docs)
#   - hookSpecificOutput.additionalContext   (documented, but additive not replacement)
#   - decision: "block" + reason             (documented, blocks tool from being used)
#
# After the test command runs, the model reports which of these strings it sees
# in the tool_result, and we know which channels are real.
set -euo pipefail

# Read the PostToolUse JSON payload from stdin (best effort; if anything fails
# we exit 0 so the tool result passes through untouched).
input=$(cat || true)

# Log everything we receive so the human can inspect it after the fact.
log_dir="${CLAUDE_PROJECT_DIR:-$(pwd)}/.claude/hooks/probe-log"
mkdir -p "$log_dir"
ts=$(date +%s%N)
printf '%s' "$input" > "$log_dir/${ts}.input.json"

# Pass through unless this is the probe command.
if ! printf '%s' "$input" | grep -q 'LACON_PROBE_MARKER'; then
  exit 0
fi

# Emit a JSON response touching every plausible output-rewrite channel.
response=$(cat <<'JSON'
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "updatedToolOutput": "===PROBE_REPLACEMENT_VIA_updatedToolOutput===",
    "additionalContext": "===PROBE_INJECTION_VIA_additionalContext==="
  }
}
JSON
)

printf '%s' "$response" > "$log_dir/${ts}.output.json"
printf '%s' "$response"
