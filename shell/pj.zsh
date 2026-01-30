# project-picker shell integration for zsh
# Add to your .zshrc: source /path/to/pj.zsh

# Navigate to project with fuzzy search
pj() {
    local selected
    selected=$(project-picker --action print --output path "$@")
    if [[ -n "$selected" ]]; then
        cd "$selected" || return 1
        echo "→ $(basename "$selected")"
    fi
}

# Open project in VS Code
pjc() {
    project-picker --action code "$@"
}

# Run dev command for selected project
pjd() {
    local cmd
    cmd=$(project-picker --action run "$@")
    if [[ -n "$cmd" ]]; then
        eval "$cmd"
    fi
}

# Ctrl+P binding (optional)
_pj_widget() {
    local selected
    selected=$(project-picker --action print --output path)
    if [[ -n "$selected" ]]; then
        BUFFER="cd ${(q)selected}"
        zle accept-line
    else
        zle reset-prompt
    fi
}
zle -N _pj_widget

# Uncomment to bind Ctrl+P:
# bindkey '^P' _pj_widget
