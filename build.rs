use std::io::Write;

/// Progenitor doesn't handle `default` error responses with `oneOf`.
/// Replace them with 4XX/5XX responses pointing to a unified ApiError schema.
fn simplify_error_responses(spec: &mut serde_json::Value) {
    spec["components"]["schemas"]["ApiError"] = serde_json::json!({
        "type": "object",
        "properties": {
            "type": { "type": "string" },
            "message": { "type": "string" },
            "statusCode": { "type": "integer" }
        },
        "required": ["type", "message", "statusCode"]
    });

    if let Some(paths) = spec["paths"].as_object_mut() {
        for (_path, methods) in paths.iter_mut() {
            if let Some(methods) = methods.as_object_mut() {
                for (_method, operation) in methods.iter_mut() {
                    if let Some(responses) = operation
                        .get_mut("responses")
                        .and_then(|r| r.as_object_mut())
                    {
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

/// The API returns `createdAt` as either a JSON string or integer depending on
/// the record. Patch the generated code to use a custom deserializer that
/// accepts both formats.
fn patch_timestamp_deserializer(content: &str) -> String {
    content.replace(
        r#"#[serde(rename = "createdAt")]
        pub created_at: i64,"#,
        r#"#[serde(rename = "createdAt", deserialize_with = "crate::api::types::deserialize_timestamp")]
        pub created_at: i64,"#,
    )
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

    let content = patch_timestamp_deserializer(&content);

    let out_file = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("codegen.rs");
    let mut f = std::fs::File::create(out_file).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}
