#!/bin/sh
# Everything the images are made from, built once, into /out.
#
# Run by the Dockerfile's builder stage, with cargo's registry and target
# directory on BuildKit cache mounts: the target directory does not survive
# the RUN that uses it, so what the runtime stages copy is copied out of it
# here, into /out.
#
# Always: the shell, and the two front ends an image is run with — the
# gallery (the image's default, every pack at once) and the demo pack (what
# the twenty-widget surface is drawn with). With TWENTY=1, also every widget
# of HLIN-I-0012: each one's module and own UI with Trunk, then its server
# with those builds embedded (its `embed` feature), so its image is one file.
#
# The widgets are found rather than listed: every folder in crates/widgets
# with a `module/`. The list the demo reads (`WIDGETS` in
# .angreal/task_demo.py) decides which of them are run.
set -eu

out=/out
mkdir -p "$out/bin"

widgets=""
if [ "${TWENTY:-0}" = "1" ]; then
  for module in crates/widgets/*/module/Cargo.toml; do
    widget=$(basename "$(dirname "$(dirname "$module")")")
    case "$widget" in hlin-widget-*) continue ;; esac
    widgets="$widgets $widget"
  done
fi

# The browser's side first, because the widget servers embed it.
#
# One `cargo build` for every WebAssembly crate at once, which compiles what
# they share once and uses every core; then one `trunk build` per crate, all
# at once, each finding the compiling done and only running wasm-bindgen and
# wasm-opt. The other way round, a Trunk per crate would queue on cargo's lock
# and compile one after another (the same order `demo up --with twenty` uses).
projects="examples/frontend-gallery examples/frontend-demo"
packages="-p frontend-gallery -p frontend-demo"
for widget in $widgets; do
  for kind in module ui; do
    if [ -f "crates/widgets/$widget/$kind/Cargo.toml" ]; then
      projects="$projects crates/widgets/$widget/$kind"
      packages="$packages -p hlin-widget-$widget-$kind"
    fi
  done
done

echo "compiling the WebAssembly: $packages"
# shellcheck disable=SC2086
cargo build --release --target wasm32-unknown-unknown $packages

pids=""
for project in $projects; do
  log="/tmp/trunk-$(echo "$project" | tr / -).log"
  (cd "$project" && trunk build --release >"$log" 2>&1) &
  pids="$pids $!:$project:$log"
done
failed=0
for entry in $pids; do
  pid=${entry%%:*}
  rest=${entry#*:}
  project=${rest%%:*}
  log=${rest#*:}
  if ! wait "$pid"; then
    echo "trunk build failed in $project:" >&2
    cat "$log" >&2
    failed=1
  fi
done
[ "$failed" = 0 ]

# Then the servers: the shell, and each widget with its builds compiled in.
servers="-p hlin"
for widget in $widgets; do
  servers="$servers -p hlin-widget-$widget --features hlin-widget-$widget/embed"
done
echo "compiling the servers: $servers"
# shellcheck disable=SC2086
cargo build --release $servers

cp target/release/hlin "$out/bin/"
for widget in $widgets; do
  cp "target/release/hlin-widget-$widget" "$out/bin/"
done
cp -R examples/frontend-gallery/dist "$out/frontend-gallery"
cp -R examples/frontend-demo/dist "$out/frontend-demo"
ls -l "$out/bin"
