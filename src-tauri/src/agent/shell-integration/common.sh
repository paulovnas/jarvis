# Metadata is emitted by shell hooks, never inferred from terminal input.
__jarvis_token='__JARVIS_TOKEN__'
typeset +x __jarvis_token 2>/dev/null || true
__jarvis_encode() {
    printf '%s' "$1" | command base64 | command tr -d '\r\n'
}
__jarvis_background_jobs() {
    local __jarvis_jobs
    __jarvis_jobs=$(jobs -pr 2>/dev/null | command wc -l | command tr -d '[:space:]')
    printf '%s' "${__jarvis_jobs:-0}"
}
__jarvis_emit() {
    case "$1" in
        start) printf '\033]777;jarvis;%s;start;%s;%s;%s\007' "$__jarvis_token" "$(__jarvis_encode "$PWD")" "$(__jarvis_encode "$2")" "${3:-1}" ;;
        end) printf '\033]777;jarvis;%s;end;%s;%s\007' "$__jarvis_token" "$2" "$(__jarvis_encode "$PWD")" ;;
        idle) printf '\033]777;jarvis;%s;idle;%s;%s\007' "$__jarvis_token" "$(__jarvis_encode "$PWD")" "$(__jarvis_background_jobs)" ;;
    esac
}
