use std::io::Write;

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

    let spec: openapiv3::OpenAPI =
        serde_json::from_reader(std::fs::File::open(src).unwrap()).unwrap();

    let mut generator = progenitor::Generator::default();
    let tokens = generator.generate_tokens(&spec).unwrap();
    let ast = syn::parse2(tokens).unwrap();
    let content = prettyplease::unparse(&ast);

    let content = patch_timestamp_deserializer(&content);

    let out_file = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("codegen.rs");
    let mut f = std::fs::File::create(out_file).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}
