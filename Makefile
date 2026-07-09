.PHONY: debug release lock clean

debug:
	cargo build

release:
	cargo build --release

lock:
	pixi lock --manifest-path locks/pixi.toml
	mv locks/pixi.lock locks/lembas.lock

clean:
	cargo clean
