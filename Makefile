.PHONY: build check

build:
	python3 scripts/package.py

check:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
	node --check web/app.js
	node --check web/panel.js
	node --check web/calendar.js
