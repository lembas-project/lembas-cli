.PHONY: debug release install-debug lock clean

debug:
	cargo build

release:
	cargo build --release

install-debug: debug
	mkdir -p ~/.local/bin
	ln -sf $(CURDIR)/target/debug/lembas ~/.local/bin/lembas

lock:
	pixi lock --manifest-path locks/pixi.toml

clean:
	cargo clean
