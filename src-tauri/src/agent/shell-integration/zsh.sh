(( ${__jarvis_zsh_installed:-0} )) && return 0
typeset -g __jarvis_zsh_installed=1 __jarvis_active=0
__jarvis_status() {
    return "$1"
}
__jarvis_preexec() {
    __jarvis_active=1
    __jarvis_emit start "$1"
}
__jarvis_precmd() {
    local __jarvis_exit=${1:-$?}
    if (( __jarvis_active )); then
        __jarvis_active=0
        __jarvis_emit end "$__jarvis_exit"
    fi
    __jarvis_emit idle
    return 0
}
# Scalar hooks run before hook arrays, and an error can suppress those arrays.
# Instrument before a scalar hook, preserving its body, arguments and status.
if (( $+functions[preexec] )); then
    functions -c preexec __jarvis_user_preexec
    function preexec() {
        local __jarvis_exit=$?
        __jarvis_preexec "$1"
        __jarvis_status "$__jarvis_exit"
        __jarvis_user_preexec "$@"
    }
    preexec_functions=(${preexec_functions:#__jarvis_preexec})
else
    preexec_functions=(__jarvis_preexec ${preexec_functions:#__jarvis_preexec})
fi
if (( $+functions[precmd] )); then
    functions -c precmd __jarvis_user_precmd
    function precmd() {
        local __jarvis_exit=$?
        __jarvis_precmd "$__jarvis_exit"
        __jarvis_status "$__jarvis_exit"
        __jarvis_user_precmd "$@"
    }
    precmd_functions=(${precmd_functions:#__jarvis_precmd})
else
    precmd_functions=(__jarvis_precmd ${precmd_functions:#__jarvis_precmd})
fi
