#!/bin/bash
#xdg-open http://localhost:3113/

# replace 'client' & 'webserver' below with the appropriate directory names for your project
client='naia-demo-macroquad-client'

get_reload_actions(){
  local OUTPUT=''
  local c=$1
  FMT='
  cargo build --target wasm32-unknown-unknown --target-dir target &&
  cd ../../../dev_http_server
  rm -rf dist &&
  mkdir dist &&
  cp ../demos/macroquad/client/target/wasm32-unknown-unknown/debug/%s.wasm dist/%s.wasm &&
  cp -a ../macroquad/client/static/. dist/ &&
  cp -a ../macroquad/client/js/. dist/ &&
  cargo run'
  printf -v OUTPUT "$FMT" $c $c
  echo $OUTPUT
}

cd demos/macroquad/client || exit
actions="$(get_reload_actions $client)"
watchexec -r -s SIGKILL --ignore dev_http_server/dist --ignore target --clear "$actions"
