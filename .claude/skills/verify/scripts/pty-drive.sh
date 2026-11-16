#!/usr/bin/env bash
# Drive a program in a detached tmux pane the way a user's terminal would, and read the screen back as text.
# Usage: pty-drive.sh <command> NAME [args]
#   start  NAME COLS ROWS [-- CMD...]   new pane running CMD (default: an interactive bash with a plain prompt)
#   keys   NAME KEY...                  tmux key names: Down Up Enter Escape C-c q ? , 1 j k
#   text   NAME STRING                  type the literal string (use for shell commands, add Enter as a key)
#   paste  NAME STRING                  paste the string as the terminal does (bracketed, when the program asked for it)
#   wheel  NAME up|down COL ROW         SGR mouse wheel event at a 1-based cell
#   click  NAME COL ROW                 SGR left click at a 1-based cell
#   resize NAME COLS ROWS
#   screen NAME                         the visible screen as text
#   info   NAME                         pane facts: size, cursor visible, alternate screen, pid, dead
#   wait   NAME NEEDLE [SECONDS]        poll the screen until NEEDLE appears (exit 1 on timeout)
#   pid    NAME                         pid of the pane's process
#   stop   NAME                         kill only this session
set -euo pipefail
SOCK=dustpan-verify
t() { tmux -L "$SOCK" -f /dev/null "$@"; }
cmd=${1:?command}
name=${2:?name}
shift 2
case $cmd in
  start)
    cols=$1 rows=$2
    shift 2
    [[ ${1:-} == -- ]] && shift
    if [[ $# -eq 0 ]]; then
      set -- env PS1='$ ' bash --norc --noprofile -i
    fi
    t kill-session -t "$name" 2>/dev/null || true
    t new-session -d -s "$name" -x "$cols" -y "$rows" "$@"
    t set-option -t "$name" window-size manual >/dev/null
    t set-option -t "$name" status off >/dev/null
    t set-option -t "$name" remain-on-exit on >/dev/null
    t resize-window -t "$name" -x "$cols" -y "$rows"
    ;;
  keys) t send-keys -t "$name" "$@" ;;
  text) t send-keys -t "$name" -l "$1" ;;
  paste)
    t set-buffer -b dpverify -- "$1"
    t paste-buffer -p -d -b dpverify -t "$name"
    ;;
  wheel)
    button=64
    [[ $1 == down ]] && button=65
    t send-keys -t "$name" -l "$(printf '\033[<%d;%d;%dM' "$button" "$2" "$3")"
    ;;
  click)
    t send-keys -t "$name" -l "$(printf '\033[<0;%d;%dM\033[<0;%d;%dm' "$1" "$2" "$1" "$2")"
    ;;
  resize) t resize-window -t "$name" -x "$1" -y "$2" ;;
  screen) t capture-pane -p -t "$name" ;;
  info) t display-message -p -t "$name" 'size=#{pane_width}x#{pane_height} cursor_visible=#{cursor_flag} alternate_screen=#{alternate_on} pid=#{pane_pid} dead=#{pane_dead} exit=#{pane_dead_status}' ;;
  pid) t display-message -p -t "$name" '#{pane_pid}' ;;
  wait)
    needle=$1 limit=${2:-5}
    for ((i = 0; i < limit * 20; i++)); do
      if t capture-pane -p -t "$name" | grep -qF -- "$needle"; then exit 0; fi
      sleep 0.05
    done
    exit 1
    ;;
  stop) t kill-session -t "$name" 2>/dev/null || true ;;
  *) echo "unknown command $cmd" >&2; exit 2 ;;
esac
