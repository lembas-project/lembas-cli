.PHONY: debug release install lock clean

debug:
	cargo build

release:
	cargo build --release

install: debug
	mkdir -p ~/.local/bin
	cp target/debug/lembas ~/.local/bin/

lock:
	pixi lock --manifest-path locks/pixi.toml
	mv locks/pixi.lock locks/lembas.lock

clean:
	cargo clean
