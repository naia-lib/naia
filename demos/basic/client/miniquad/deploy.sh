#!/bin/bash
xdg-open http://localhost:3113/ # This will open a browser on Linux
open http://localhost:3113/ # This will open a browser on macOS

# replace 'client' & 'directory' below with the appropriate directory names for your project
client='naia-basic-client-demo-mq'
directory='basic/client/miniquad'

get_reload_actions(){
  local OUTPUT=''
  local c=$1
  local d=$2
  FMT='
  cargo build --target wasm32-unknown-unknown --target-dir target;
  cd ../../../demo_utils/dev_http_server;
  rm -rf dist;
  mkdir dist;
  cp ../../%s/target/wasm32-unknown-unknown/debug/%s.wasm dist/%s.wasm;
  cp -a ../../%s/static/. dist/;
  cp -a ../../%s/js/. dist/;
  cargo run'
  printf -v OUTPUT "$FMT" $d $c $c $d $d
  echo $OUTPUT
}

cd demos/basic/client/miniquad || exit
actions="$(get_reload_actions $client $directory)"
watchexec -r -s SIGKILL --ignore dev_http_server/dist --ignore target --clear "$actions"
