#!/usr/bin/env bash
# The shell reaches the Host through the control interface only (RFC 005 rule 25; the
# handoff's cross-cutting rule): the page's loader may call the worker module's transport
# entries and the display path's, never a run command of its own. Every `florn_*` name the
# loader and the shell mention must be on the list below.
set -euo pipefail
cd "$(dirname "$0")/.."
allowed='florn_start|florn_handle_bytes|florn_pump_bytes|florn_drain_bytes|florn_take_display|florn_display_ptr|florn_display_dropped|florn_alloc|florn_out_ptr|florn_out_len|florn_shell|florn_host|florn_control|florn_ui_model|florn_ui_scene|florn_ui_web'
status=0
for f in crates/florn-ui-web/www/app.js crates/florn-ui-web/src/shell/*.rs; do
  if grep -oE 'florn_[a-z_]+' "$f" | sort -u | grep -vE "^($allowed)" ; then
    echo "a worker export beyond the control interface is used in $f"; status=1
  fi
done
exit $status
