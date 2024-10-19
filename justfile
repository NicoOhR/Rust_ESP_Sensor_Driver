set shell := ["fish", "-c"]

build:
  . $HOME/scripts/export-esp.sh
  cargo build --release

