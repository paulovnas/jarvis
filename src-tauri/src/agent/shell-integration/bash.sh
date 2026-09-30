# An existing DEBUG trap may own debugger/readline behavior. Leave it intact.
if [[ -n $(trap -p DEBUG) ]]; then
    return 0
fi
__jarvis_active=0
__jarvis_ready=0
__jarvis_line_seen=0
__jarvis_full_line=''
__jarvis_history_command() {
    # History is only compared with the command observed at this real boundary;
    # it never supplies a command to reconstruct or replay. Common ignoreboth
    # settings still allow ordinary simple commands to be recognized.
    [[ -o history && ${HISTSIZE:-0} =~ ^[1-9][0-9]*$ ]] || return 0
    local __jarvis_history_line
    __jarvis_history_line=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null) || return 0
    [[ $__jarvis_history_line =~ ^[[:space:]]*[0-9]+[[:space:]]+(.*)$ ]] || return 0
    __jarvis_history_line=${BASH_REMATCH[1]}
    while [[ $__jarvis_history_line == ' '* || $__jarvis_history_line == $'\t'* ]]; do
        __jarvis_history_line=${__jarvis_history_line:1}
    done
    printf '%s' "$__jarvis_history_line"
}
__jarvis_debug() {
    if [[ $__jarvis_ready == 1 && ${BASH_SUBSHELL:-0} == 0 && ${#FUNCNAME[@]} == 1 && $1 != __jarvis_* ]]; then
        if [[ $__jarvis_line_seen == 0 ]]; then
            __jarvis_full_line=$(__jarvis_history_command)
            __jarvis_line_seen=1
        fi
        local __jarvis_restartable=0
        if [[ -n $__jarvis_full_line && $__jarvis_full_line == "$1" ]]; then
            __jarvis_restartable=1
        fi
        __jarvis_active=1
        # Every top-level command replaces the previous one, including cwd.
        __jarvis_emit start "$1" "$__jarvis_restartable"
    fi
    return 0
}
__jarvis_precmd() {
    local __jarvis_exit=$?
    __jarvis_ready=0
    __jarvis_line_seen=0
    __jarvis_full_line=''
    if [[ $__jarvis_active == 1 ]]; then
        __jarvis_active=0
        __jarvis_emit end "$__jarvis_exit"
    fi
    __jarvis_emit idle
    return "$__jarvis_exit"
}
if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == 'declare -a '* ]]; then
    PROMPT_COMMAND=(__jarvis_precmd "${PROMPT_COMMAND[@]}" '__jarvis_ready=1')
else
    PROMPT_COMMAND="__jarvis_precmd${PROMPT_COMMAND:+; $PROMPT_COMMAND}; __jarvis_ready=1"
fi
trap '__jarvis_debug "$BASH_COMMAND"' DEBUG
