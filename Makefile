.PHONY: check check-openapi check-help generate generate-openapi generate-help

check: check-openapi check-help

check-openapi:
	@echo "Checking vendored OpenAPI spec matches upstream..."
	@curl -sf https://api.detail.dev/public/v1/openapi.json \
		| python3 -m json.tool \
		| diff - openapi.json

check-help:
	@echo "Checking docs/HELP.md is up to date..."
	@cargo run --quiet --example generate_help | diff - docs/HELP.md

generate: generate-openapi generate-help

generate-openapi:
	curl -sf https://api.detail.dev/public/v1/openapi.json \
		| python3 -m json.tool > openapi.json

generate-help:
	cargo run --quiet --example generate_help > docs/HELP.md
