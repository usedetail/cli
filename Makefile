.PHONY: generate generate-help generate-openapi check check-help check-openapi

## Generate all vendored/generated files
generate: generate-help generate-openapi

## Regenerate docs/HELP.md from Clap definitions
generate-help:
	cargo run --example generate_help > docs/HELP.md

## Fetch the latest OpenAPI spec from upstream
generate-openapi:
	curl -sf https://api.detail.dev/public/v1/openapi.json \
		| python3 -m json.tool > openapi.json

## Verify all generated files are up to date
check: check-help check-openapi

## Verify docs/HELP.md matches Clap definitions
check-help:
	cargo run --example generate_help | diff - docs/HELP.md

## Verify vendored OpenAPI spec matches upstream
check-openapi:
	curl -sf https://api.detail.dev/public/v1/openapi.json \
		| python3 -m json.tool | diff - openapi.json
