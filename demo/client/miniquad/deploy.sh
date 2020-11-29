#!/bin/bash
xdg-open http://localhost:3113/

# replace 'client' & 'webserver' below with the appropriate directory names for your project
client='naia-client-miniquad-example'
webserver_dir='dev_http_server'

get_reload_actions(){
  local OUTPUT=''
  local c=$1
  local w=$2
  FMT='rm -rf %s/dist &&
  mkdir %s/dist &&
  cargo build --target wasm32-unknown-unknown --features mquad --target-dir target &&
  cp target/wasm32-unknown-unknown/debug/%s.wasm %s/dist/%s.wasm &&
  cp -a static/. %s/dist/ &&
  cp -a js/. %s/dist/ &&
  cd %s &&
  cargo run'
  printf -v OUTPUT "$FMT" $w $w $c $w $c $w $w $w
  echo $OUTPUT
}

cd demo/client/miniquad || exit
actions="$(get_reload_actions $client $webserver_dir)"
watchexec -r -s SIGKILL --ignore $webserver_dir/dist --ignore target --clear "$actions"
