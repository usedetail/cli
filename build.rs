use std::io::Write;

/// Progenitor doesn't handle `default` error responses with `anyOf`.
/// Replace them with a single unified ApiError schema.
fn simplify_error_responses(spec: &mut serde_json::Value) {
    // Add a generic ApiError to components/schemas
    spec["components"]["schemas"]["ApiError"] = serde_json::json!({
        "type": "object",
        "properties": {
            "type": { "type": "string" },
            "message": { "type": "string" },
            "statusCode": { "type": "integer" }
        },
        "required": ["type", "message", "statusCode"]
    });

    // Replace every `default` error response with specific 4xx/5xx using ApiError
    if let Some(paths) = spec["paths"].as_object_mut() {
        for (_path, methods) in paths.iter_mut() {
            if let Some(methods) = methods.as_object_mut() {
                for (_method, operation) in methods.iter_mut() {
                    if let Some(responses) = operation.get_mut("responses").and_then(|r| r.as_object_mut()) {
                        if responses.remove("default").is_some() {
                            let error_resp = serde_json::json!({
                                "description": "Error",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$ref": "#/components/schemas/ApiError"
                                        }
                                    }
                                }
                            });
                            responses.insert("4XX".to_string(), error_resp.clone());
                            responses.insert("5XX".to_string(), error_resp);
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    let src = "openapi.json";
    println!("cargo::rerun-if-changed={src}");

    let file = std::fs::File::open(src).unwrap();
    let mut spec: serde_json::Value = serde_json::from_reader(file).unwrap();

    simplify_error_responses(&mut spec);

    let spec: openapiv3::OpenAPI = serde_json::from_value(spec).unwrap();
    let mut generator = progenitor::Generator::default();

    let tokens = generator.generate_tokens(&spec).unwrap();
    let ast = syn::parse2(tokens).unwrap();
    let content = prettyplease::unparse(&ast);

    let out_file = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("codegen.rs");
    let mut f = std::fs::File::create(out_file).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}
