.PHONY: install
install:
	cargo build --release
	spin pluginify -i 