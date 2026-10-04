# ostrov's completion for bash, answered by the running ostrov (`ostrov complete WORD...`: a line a candidate,
# its description after a tab, unused here); nothing while none runs.

_ostrov() {
  local cur words cword line
  if declare -F _get_comp_words_by_ref > /dev/null; then
    _get_comp_words_by_ref -n : cur words cword
  else
    cur=${COMP_WORDS[COMP_CWORD]} words=("${COMP_WORDS[@]}") cword=$COMP_CWORD
  fi
  COMPREPLY=()
  while IFS= read -r line; do
    COMPREPLY+=("$(printf '%q' "${line%%$'\t'*}")")
  done < <(ostrov complete "${words[@]:1:cword}" 2>/dev/null)
  if declare -F __ltrim_colon_completions > /dev/null; then
    __ltrim_colon_completions "$cur"
  fi
}

complete -F _ostrov ostrov
